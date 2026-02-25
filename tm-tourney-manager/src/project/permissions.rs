use std::ops::{Add, BitAnd, BitOr, Not};

use crate::authorization::PermissionType;

/* #[spacetimedb::table(accessor= tab_project_permission, index(name= account_and_project, hash(columns=[account_id,project_id])))]
pub struct ProjectPermissionV1 {
    pub project_id: u32,

    bucket1: u64,

    account_id: Uuid,
}

impl ProjectPermissionV1 {
    pub(crate) fn get_permissions(&self) -> ProjectPermissionsV1 {
        ProjectPermissionsV1(self.bucket1)
    }
} */

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub(crate) struct ProjectPermissionsV1(pub(super) u64);

impl ProjectPermissionsV1 {
    pub const OWNER: ProjectPermissionsV1 = ProjectPermissionsV1(0b1);

    pub const PROJECT_EDIT_NAME: ProjectPermissionsV1 = ProjectPermissionsV1(0b10);
    pub const PROJECT_EDIT_DATE: ProjectPermissionsV1 = ProjectPermissionsV1(0b100);
    pub const PROJECT_EDIT_DESCRIPTION: ProjectPermissionsV1 = ProjectPermissionsV1(0b1000);

    pub const COMPETITION_CREATE: ProjectPermissionsV1 = ProjectPermissionsV1(0b100000);
    pub const COMPETITION_EDIT_NAME: ProjectPermissionsV1 = ProjectPermissionsV1(0b1000000);
    pub const COMPETITION_DELETE: ProjectPermissionsV1 = ProjectPermissionsV1(0b10000000);
    pub const COMPETITION_CONNECTION_EDIT: ProjectPermissionsV1 = ProjectPermissionsV1(0b100000000);
    pub const COMPETITION_LAYOUT_EDIT: ProjectPermissionsV1 = ProjectPermissionsV1(0b10000000000);
    pub const COMPETITION_EDIT_REGISTRATION: ProjectPermissionsV1 =
        ProjectPermissionsV1(0b100000000000);

    pub const MATCH_CREATE: ProjectPermissionsV1 = ProjectPermissionsV1(0b10000);
    pub const MATCH_DELETE: ProjectPermissionsV1 = ProjectPermissionsV1(0b1000000000);
    pub const MATCH_CONFIGURE: ProjectPermissionsV1 = ProjectPermissionsV1(0b1000000000000);

    pub const RAW_SERVER_ADD: ProjectPermissionsV1 = ProjectPermissionsV1(0b10000000000000);
    pub const RAW_SERVER_REVOKE: ProjectPermissionsV1 = ProjectPermissionsV1(0b100000000000000);

    pub const MATCH_ASSIGN_SERVER: ProjectPermissionsV1 = ProjectPermissionsV1(0b1000000000000000);
}

impl PermissionType for ProjectPermissionsV1 {
    fn initial() -> Self {
        Self(0)
    }

    fn passed(self) -> bool {
        self.0 == 0
    }
}

impl Add for ProjectPermissionsV1 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        ProjectPermissionsV1(self.0 + rhs.0)
    }
}

impl BitAnd for ProjectPermissionsV1 {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        ProjectPermissionsV1(self.0 & rhs.0)
    }
}

impl Not for ProjectPermissionsV1 {
    type Output = Self;

    fn not(self) -> Self::Output {
        ProjectPermissionsV1(!self.0)
    }
}

impl BitOr for ProjectPermissionsV1 {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        ProjectPermissionsV1(self.0 | rhs.0)
    }
}
