use spacetimedb::{CtxDbRead, CtxWithSender, Identity};

use crate::{
    competition::{CompetitionPermissionsV1, roles::CompetitionPermissionRead},
    raw_server::TabRawServerRead,
    user::UserRead,
};

pub(crate) trait Authorization<Db: CtxWithSender + CtxDbRead> {
    fn user_id(&self) -> Result<u32, String>;

    fn server_id(&self) -> Result<u32, String>;

    fn auth_builder(&'_ self, competition_id: u32) -> AuthBuilder<'_, Db>;
}

impl<Db: CtxWithSender + CtxDbRead> Authorization<Db> for Db {
    fn server_id(&self) -> Result<u32, String> {
        self.get_raw_server_id(self.sender())
    }

    fn auth_builder(&'_ self, competition_id: u32) -> AuthBuilder<'_, Db> {
        AuthBuilder::new(competition_id, self, self.sender())
    }

    fn user_id(&self) -> Result<u32, String> {
        self.get_user_id(self.sender())
    }
}

pub(crate) struct AuthBuilder<'a, Db: CtxDbRead> {
    expected: CompetitionPermissionsV1,
    competition_id: u32,
    db: &'a Db,
    sender: Identity,
}

impl<'a, Db: CtxDbRead> AuthBuilder<'a, Db> {
    fn new(competition_id: u32, db: &'a Db, sender: Identity) -> Self {
        AuthBuilder {
            expected: CompetitionPermissionsV1::NONE,
            competition_id,
            db,
            sender,
        }
    }

    pub(crate) fn permission(mut self, permission: CompetitionPermissionsV1) -> Self {
        self.expected = self.expected | permission;
        self
    }

    pub(crate) fn authorize(self) -> Result<u32, String> {
        let user_id = self.db.get_user_id(self.sender)?;

        // Membership is always required, even when no permission is expected.
        match self.db.competition_access(self.competition_id, user_id) {
            None => Err("You are not a member of this competition.".into()),
            Some(permissions) if permissions.has(self.expected) => Ok(user_id),
            Some(_) => Err("Not sufficient permissions to perform this action.".into()),
        }
    }
}
