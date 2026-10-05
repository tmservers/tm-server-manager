use std::ops::{Add, BitAnd, BitOr, Not};

use crate::authorization::PermissionType;

#[derive(Debug, PartialEq, Eq, Clone, Copy, Default)]
pub(crate) struct CompetitionPermissionsV1(pub(super) u64);

impl CompetitionPermissionsV1 {
    pub const NONE: CompetitionPermissionsV1 = CompetitionPermissionsV1(0);

    /// This is a special Permission which implicitly acts as a "Role".
    pub const OWNER: CompetitionPermissionsV1 = CompetitionPermissionsV1(1);

    pub const COMPETITION_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 4);
    pub const COMPETITION_EDIT_NAME: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 5);
    pub const COMPETITION_DELETE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 6);
    pub const COMPETITION_CONNECTION_EDIT: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(1 << 7);
    pub const COMPETITION_LAYOUT_EDIT: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 18);

    pub const MATCH_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 10);
    pub const MATCH_DELETE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 11);
    pub const MATCH_CONFIGURE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 12);

    pub const RAW_SERVER_ADD: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 13);
    pub const RAW_SERVER_REVOKE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 14);

    pub const MATCH_ASSIGN_SERVER: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 15);

    pub const REGISTRATION_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 16);

    pub const SCHEDULE_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 17);

    pub const INPUT_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 21);

    pub const SERVER_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 19);
    pub const OUTPUT_CREATE: CompetitionPermissionsV1 = CompetitionPermissionsV1(1 << 20);

    pub const TRACKMANIA_SPECTATE_MATCHES: CompetitionPermissionsV1 =
        CompetitionPermissionsV1(1 << 21);

    pub(crate) fn has(self, perm: Self) -> bool {
        self.bypass() || (perm & !self) == CompetitionPermissionsV1::NONE
    }
}

impl PermissionType for CompetitionPermissionsV1 {
    fn initial() -> Self {
        Self(0)
    }

    fn bypass(self) -> bool {
        // Owner bypass
        if get_bit_at(self.0, 0) {
            return true;
        }

        false
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

fn get_bit_at(input: u64, n: u8) -> bool {
    if n < 64 { input & (1 << n) != 0 } else { false }
}
