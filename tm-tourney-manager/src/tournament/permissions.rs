use std::ops::Add;

use spacetimedb::Uuid;

use crate::{
    authorization::{AuthBuilder, PermissionType},
    user::UserV1,
};

#[spacetimedb::table(name = tab_tournament_permission, index(name= account_and_tournament, hash(columns=[account_id,tournament_id])))]
pub struct TournamentPermissionV1 {
    pub tournament_id: u32,

    permission_bucket: u64,

    account_id: Uuid,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) struct TournamentPermissionsV1(u64);

impl TournamentPermissionsV1 {
    // This would be a role but how to model that
    //pub const CREATOR: TournamentPermissionsV1 = TournamentPermissionsV1(0b1);

    pub const EDIT_NAME: TournamentPermissionsV1 = TournamentPermissionsV1(0b10);
}

impl PermissionType for TournamentPermissionsV1 {
    fn initial() -> Self {
        Self(0)
    }

    fn evaluate(self) -> Result<(), String> {
        /* match self {
            Self::CREATOR => Ok(()),

            _ => Err("TODO".into()),
        } */

        Ok(())
    }
}

impl Add for TournamentPermissionsV1 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        TournamentPermissionsV1(self.0 + rhs.0)
    }
}

/* impl TournamentPermissionV1 {
    const CREATOR: u32 = 0b1;

    //pub(crate) fn authorize(&self, user: &UserV1) -> AuthBuilder<TournamentPermissionsV1> {};
}
 */
