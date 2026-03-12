use std::ops::{Add, BitAnd, BitOr, Not};

use crate::authorization::PermissionType;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub(crate) struct CompetitionPermissionsV1(pub(super) u64);

impl CompetitionPermissionsV1 {
    pub const OWNER: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b1);

    //pub const PROJECT_EDIT_NAME: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b10);
    //pub const PROJECT_EDIT_DATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b100);
    //pub const PROJECT_EDIT_DESCRIPTION: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b1000);

    pub const COMPETITION_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b100000);
    pub const COMPETITION_EDIT_NAME: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b1000000);
    pub const COMPETITION_DELETE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b10000000);
    pub const COMPETITION_CONNECTION_EDIT: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b100000000);
    pub const COMPETITION_LAYOUT_EDIT: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b10000000000);
    pub const COMPETITION_EDIT_REGISTRATION: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b100000000000);

    pub const MATCH_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b10000);
    pub const MATCH_DELETE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b1000000000);
    pub const MATCH_CONFIGURE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b1000000000000);

    pub const RAW_SERVER_ADD: CompetitionPermissionsV1 = CompetitionPermissionsV1(0b10000000000000);
    pub const RAW_SERVER_REVOKE: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b100000000000000);

    pub const MATCH_ASSIGN_SERVER: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b1000000000000000);

    pub const REGISTRATION_CREATE: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(0b10000000000000000);
}

impl PermissionType for CompetitionPermissionsV1 {
    fn initial() -> Self {
        Self(0)
    }

    fn passed(self) -> bool {
        //TODO make this correct.
        //TODO how to ideally handle the OWNER
        self.0 == 0
    }

    fn bypass(&self) -> bool {
        //TODO make this correct
        // self.0 & 0b1 == 0b1
        true
    }
}

impl Add for CompetitionPermissionsV1 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        CompetitionPermissionsV1(self.0 + rhs.0)
    }
}

impl BitAnd for CompetitionPermissionsV1 {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        CompetitionPermissionsV1(self.0 & rhs.0)
    }
}

impl Not for CompetitionPermissionsV1 {
    type Output = Self;

    fn not(self) -> Self::Output {
        CompetitionPermissionsV1(!self.0)
    }
}

impl BitOr for CompetitionPermissionsV1 {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        CompetitionPermissionsV1(self.0 | rhs.0)
    }
}
