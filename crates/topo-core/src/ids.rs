//! Typed arena indices. Each id is the index of the entity in its `Model` vector.

use serde::{Deserialize, Serialize};
use std::fmt;

macro_rules! id_type {
    ($name:ident, $prefix:literal) => {
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(pub u32);

        impl $name {
            pub fn idx(self) -> usize {
                self.0 as usize
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($prefix, "{}"), self.0)
            }
        }
    };
}

id_type!(NodeId, "N");
id_type!(MemberId, "M");
id_type!(GroupId, "G");
id_type!(SectionId, "SEC");
id_type!(MaterialId, "MAT");
id_type!(ConnectionId, "C");
id_type!(LoadCaseId, "LC");
