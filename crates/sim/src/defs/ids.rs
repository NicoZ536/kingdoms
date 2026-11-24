//! Compact typed ids for data definitions.
//!
//! Every id is an index into the corresponding `Vec` inside [`Defs`](super::Defs). Ids are only
//! created by the loader, so an id obtained from a `Defs` is always valid for that `Defs`. Ids
//! stored in a save game are tied to the data set via [`Defs::fingerprint`](super::Defs::fingerprint).

use std::fmt;

use serde::{Deserialize, Serialize};

macro_rules! def_id {
    ($(#[$meta:meta])* $name:ident($repr:ty), $label:literal) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name($repr);

        impl $name {
            /// Index into the definition list of [`Defs`](super::Defs).
            pub const fn index(self) -> usize {
                self.0 as usize
            }

            /// Creates an id from a list index; `None` if it does not fit the id's integer type.
            pub(crate) fn from_index(index: usize) -> Option<Self> {
                <$repr>::try_from(index).ok().map(Self)
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, concat!($label, "#{}"), self.0)
            }
        }
    };
}

def_id!(
    /// Id of a [`ResourceDef`](super::ResourceDef).
    ResourceId(u16),
    "resource"
);
def_id!(
    /// Id of a [`BuildingDef`](super::BuildingDef).
    BuildingTypeId(u16),
    "building"
);
def_id!(
    /// Id of a [`RoadDef`](super::RoadDef).
    RoadTypeId(u8),
    "road"
);
def_id!(
    /// Id of a [`CropDef`](super::CropDef).
    CropId(u8),
    "crop"
);
def_id!(
    /// Id of a [`TechDef`](super::TechDef).
    TechId(u16),
    "tech"
);
def_id!(
    /// Id of a [`UnitDef`](super::UnitDef).
    UnitTypeId(u8),
    "unit"
);
def_id!(
    /// Id of an [`EventDef`](super::EventDef).
    EventDefId(u16),
    "event"
);
def_id!(
    /// Id of a [`ScenarioDef`](super::ScenarioDef).
    ScenarioId(u8),
    "scenario"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_index_and_overflow() {
        assert_eq!(ResourceId::from_index(7).map(ResourceId::index), Some(7));
        assert!(RoadTypeId::from_index(255).is_some());
        assert!(RoadTypeId::from_index(256).is_none());
        assert_eq!(BuildingTypeId::from_index(3).map(|b| b.to_string()), Some("building#3".to_string()));
    }

    #[test]
    fn ids_serialize_as_plain_numbers() {
        let id = TechId::from_index(12).unwrap();
        assert_eq!(ron::to_string(&id).unwrap(), "12");
        let back: TechId = ron::from_str("12").unwrap();
        assert_eq!(back, id);
    }
}
