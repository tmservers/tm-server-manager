use std::collections::HashSet;

use spacetimedb::{CtxDbRead, ReducerContext, Table, ViewContext, reducer, table, view};

use crate::{
    authorization::Authorization,
    competition::{
        CompetitionPermissionsV1,
        node::{NodeHandle, NodeRead},
    },
    pipeline::{
        LbEntry, LbParticipant, context::LbContext, error::LbError, modes::match_standings,
    },
    tm_match::{MatchRead, MatchStatus},
};

/// A participant of a match with its seed.
/// The entrants get frozen when the match gets prepared, so they stay the same while it is played.
#[table(accessor = tab_match_entrant, vis_private)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MatchEntrant {
    #[index(hash)]
    match_id: u32,
    /// 0 for the entrant of a team.
    pub user_id: u32,
    pub team_id: u32,
    /// Starting at 1.
    pub seed: u16,
}

impl MatchEntrant {
    fn new(match_id: u32, participant: LbParticipant, seed: u16) -> Self {
        let (user_id, team_id) = match participant {
            LbParticipant::Player(user_id) => (user_id, 0),
            LbParticipant::Team(team_id) => (0, team_id),
        };
        MatchEntrant {
            match_id,
            user_id,
            team_id,
            seed,
        }
    }
}

/// The participants of the inputs, which come in the order of their slots.
/// Every input keeps its own order and someone in several inputs keeps the best seed.
fn entrant_order(inputs: Vec<Vec<LbEntry>>) -> Vec<LbParticipant> {
    let mut seen = HashSet::new();
    inputs
        .into_iter()
        .flatten()
        .map(|e| e.participant())
        .filter(|participant| seen.insert(*participant))
        .collect()
}

/// The entrants in a new order, `order` lists the current seeds starting with the one which becomes the first.
fn reseeded(entrants: &[MatchEntrant], order: &[u16]) -> Result<Vec<MatchEntrant>, String> {
    if order.len() != entrants.len() {
        return Err(format!(
            "Expected all {} seeds in the new order but got {}.",
            entrants.len(),
            order.len()
        ));
    }

    let mut given = HashSet::new();
    order
        .iter()
        .zip(1u16..)
        .map(|(&current, seed)| {
            let Some(found) = entrants.iter().find(|e| e.seed == current) else {
                return Err(format!("Seed {current} does not exist."));
            };
            if !given.insert(current) {
                return Err(format!("Seed {current} is given more than once."));
            }
            Ok(MatchEntrant { seed, ..*found })
        })
        .collect()
}

pub(crate) trait MatchEntrantRead {
    /// The entrants the match would get from its inputs right now.
    fn match_entrants_preview(&self, match_id: u32) -> Result<Vec<MatchEntrant>, LbError>;
    /// The frozen entrants of the match, the best seed first.
    fn match_entrants(&self, match_id: u32) -> Vec<MatchEntrant>;
}
impl<Db: CtxDbRead> MatchEntrantRead for Db {
    fn match_entrants_preview(&self, match_id: u32) -> Result<Vec<MatchEntrant>, LbError> {
        let node = NodeHandle::MatchV1(match_id);
        let cx = LbContext::new(self, node);
        let inputs = self
            .node_resolve_inputs(node)?
            .into_iter()
            .map(|input| match_standings(&cx, input))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(entrant_order(inputs)
            .into_iter()
            .zip(1u16..)
            .map(|(participant, seed)| MatchEntrant::new(match_id, participant, seed))
            .collect())
    }

    fn match_entrants(&self, match_id: u32) -> Vec<MatchEntrant> {
        let mut entrants: Vec<MatchEntrant> = self
            .db_read_only()
            .tab_match_entrant()
            .match_id()
            .filter(match_id)
            .collect();
        entrants.sort_by_key(|e| e.seed);
        entrants
    }
}

pub(crate) trait MatchEntrantWrite: MatchEntrantRead {
    /// Freezes the entrants the match gets from its inputs, entrants which already got frozen are kept.
    fn match_entrants_freeze(&self, match_id: u32) -> Result<(), String>;
    fn match_entrants_delete(&self, match_id: u32);
}
impl<Db: spacetimedb::CtxDbWrite> MatchEntrantWrite for Db {
    fn match_entrants_freeze(&self, match_id: u32) -> Result<(), String> {
        if !self.match_entrants(match_id).is_empty() {
            return Ok(());
        }
        for entrant in self.match_entrants_preview(match_id)? {
            self.db().tab_match_entrant().try_insert(entrant)?;
        }
        Ok(())
    }

    fn match_entrants_delete(&self, match_id: u32) {
        self.db().tab_match_entrant().match_id().delete(match_id);
    }
}

/// Reorders the frozen entrants of a match while it gets prepared.
/// `order` lists the current seeds starting with the one which becomes the first.
#[reducer]
fn match_reseed(ctx: &ReducerContext, match_id: u32, order: Vec<u16>) -> Result<(), String> {
    let Some(tm_match) = ctx.match_find(match_id) else {
        return Err("Match not found.".into());
    };
    ctx.auth_builder(tm_match.get_comp_id())
        .permission(CompetitionPermissionsV1::MATCH_RESEED)
        .authorize()?;

    if tm_match.status() != MatchStatus::Preparation {
        return Err("Seeds can only be changed while the match gets prepared.".into());
    }

    let entrants = reseeded(&ctx.match_entrants(match_id), &order)?;
    ctx.db.tab_match_entrant().match_id().delete(match_id);
    for entrant in entrants {
        ctx.db.tab_match_entrant().insert(entrant);
    }

    Ok(())
}

fn is_member(ctx: &ViewContext, match_id: u32) -> bool {
    ctx.node_get_parent(NodeHandle::MatchV1(match_id))
        .is_ok_and(|parent| ctx.auth_builder(parent).authorize().is_ok())
}

/// The frozen entrants of the match.
#[view(accessor = match_entrants, public)]
fn match_entrants(ctx: &ViewContext, match_id: u32) -> Vec<MatchEntrant> {
    if !is_member(ctx, match_id) {
        return Vec::new();
    }
    ctx.match_entrants(match_id)
}

/// The entrants the match would get from its inputs right now.
#[view(accessor = match_entrants_preview, public)]
fn match_entrants_preview(ctx: &ViewContext, match_id: u32) -> Vec<MatchEntrant> {
    if !is_member(ctx, match_id) {
        return Vec::new();
    }
    ctx.match_entrants_preview(match_id)
        .unwrap_or_else(|error| {
            log::error!("The entrants of match {match_id} could not be resolved: {error}");
            Vec::new()
        })
}
