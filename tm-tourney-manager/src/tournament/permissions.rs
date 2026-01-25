use std::ops::{Add, BitAnd, Not};

use spacetimedb::Uuid;

use crate::{
    authorization::{AuthBuilder, PermissionType},
    user::UserV1,
};

#[spacetimedb::table(name = tab_tournament_permission, index(name= account_and_tournament, hash(columns=[account_id,tournament_id])))]
pub struct TournamentPermissionV1 {
    pub tournament_id: u32,
    
    bucket1: u64,
    
    account_id: Uuid,
}

impl TournamentPermissionV1 {
    pub(crate) fn get_permissions(&self) -> TournamentPermissionsV1 {
        TournamentPermissionsV1(self.bucket1)
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct TournamentPermissionsV1(u64);

impl TournamentPermissionsV1 {
    pub const TOURNAMENT_EDIT_NAME: TournamentPermissionsV1 = TournamentPermissionsV1(0b10);
    pub const TOURNAMENT_EDIT_DATE: TournamentPermissionsV1 = TournamentPermissionsV1(0b100);
    pub const TOURNAMENT_EDIT_DESCRIPTION: TournamentPermissionsV1 =
        TournamentPermissionsV1(0b1000);

    pub const COMPETITION_CREATE: TournamentPermissionsV1 = TournamentPermissionsV1(0b100000);
    pub const COMPETITION_EDIT_NAME: TournamentPermissionsV1 = TournamentPermissionsV1(0b1000000);
    pub const COMPETITION_DELETE: TournamentPermissionsV1 = TournamentPermissionsV1(0b10000000);
    pub const COMPETITION_CONNECTION_CREATE: TournamentPermissionsV1 =
        TournamentPermissionsV1(0b100000000);
    pub const COMPETITION_CONNECTION_DELETE: TournamentPermissionsV1 =
        TournamentPermissionsV1(0b1000000000);
    pub const COMPETITION_LAYOUT_EDIT: TournamentPermissionsV1 =
        TournamentPermissionsV1(0b10000000000);

    pub const MATCH_CREATE: TournamentPermissionsV1 = TournamentPermissionsV1(0b10000);
    pub const MATCH_DELETE: TournamentPermissionsV1 = TournamentPermissionsV1(0b100000000);
}

impl PermissionType for TournamentPermissionsV1 {
    fn initial() -> Self {
        Self(0)
    }

    fn passed(self) -> bool {
        self.0 == 0
    }
}

impl Add for TournamentPermissionsV1 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        TournamentPermissionsV1(self.0 + rhs.0)
    }
}

impl BitAnd for TournamentPermissionsV1 {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        TournamentPermissionsV1(self.0 & rhs.0)
    }
}

impl Not for TournamentPermissionsV1 {
    type Output = Self;

    fn not(self) -> Self::Output {
        TournamentPermissionsV1(!self.0)
    }
}
