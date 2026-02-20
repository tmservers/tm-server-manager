use spacetimedb::{ReducerContext, Uuid, table};

use crate::tournament::permissions::TournamentPermissionsV1;

#[table(name= tab_project_role)]
pub struct ProjectRole {
    name: String,

    #[auto_inc]
    #[primary_key]
    pub(crate) id: u32,

    #[index(hash)]
    project_id: u32,

    permissions1: u64,
}

impl ProjectRole {
    pub fn get_permissions1(&self) -> TournamentPermissionsV1 {
        TournamentPermissionsV1(self.permissions1)
    }
}

#[table(name= tab_project_role_members,index(name= user_roles , hash(columns= [role_id,account_id])))]
pub struct ProjectRoleMember {
    role_id: u32,

    account_id: Uuid,
}

impl ProjectRoleMember {
    pub fn get_role_id(&self) -> u32 {
        self.role_id
    }
}
