//! Fixed, real-venue room addresses.
//!
//! The two capitals are peers, and so is every sibling under them: a district
//! does not rank above another district, and a room is a coordinate in a fixed
//! inventory rather than a resource its tenant invents. Rooms can be created
//! fresh or lit from the existing dark inventory.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::CoreError;

/// The two peer capitals of the venue map.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capital {
    Tokyo,
    Kyoto,
    /// Synthetic namespace for 4-digit pairing rooms. `南极1234` is not a
    /// physical venue and never consumes an inventory room.
    Antarctica,
}

impl Capital {
    pub const ALL: [Capital; 2] = [Capital::Tokyo, Capital::Kyoto];

    pub const fn as_str(self) -> &'static str {
        match self {
            Capital::Tokyo => "东京",
            Capital::Kyoto => "西京",
            Capital::Antarctica => "南极",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        if name == "南极" {
            return Some(Capital::Antarctica);
        }
        Self::ALL
            .into_iter()
            .find(|capital| capital.as_str() == name)
    }
}

impl fmt::Display for Capital {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One room in the fixed inventory, e.g. `东京A区1B级103房`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct RoomAddress {
    pub capital: Capital,
    pub district: String,
    pub level: String,
    pub room: String,
}

impl RoomAddress {
    pub fn new(
        capital: Capital,
        district: impl Into<String>,
        level: impl Into<String>,
        room: impl Into<String>,
    ) -> Result<Self, CoreError> {
        let address = Self {
            capital,
            district: district.into(),
            level: level.into(),
            room: room.into(),
        };
        address.validate()?;
        Ok(address)
    }

    /// The four segments in order, ready to build an auth path from. The path
    /// scheme itself is a server concern and is deliberately not fixed here.
    pub fn segments(&self) -> [&str; 4] {
        [
            self.capital.as_str(),
            &self.district,
            &self.level,
            &self.room,
        ]
    }

    fn validate(&self) -> Result<(), CoreError> {
        for (name, part) in [
            ("district", self.district.as_str()),
            ("level", self.level.as_str()),
            ("room", self.room.as_str()),
        ] {
            if part.is_empty() {
                return Err(CoreError::Address(format!("{name} is empty")));
            }
            if part
                .chars()
                .any(|c| c.is_whitespace() || matches!(c, '区' | '级' | '房' | '京'))
            {
                return Err(CoreError::Address(format!(
                    "{name} contains a reserved character"
                )));
            }
        }
        Ok(())
    }
}

impl fmt::Display for RoomAddress {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.capital == Capital::Antarctica {
            return write!(f, "南极{}", self.district);
        }
        write!(
            f,
            "{}{}区{}级{}房",
            self.capital, self.district, self.level, self.room
        )
    }
}

impl FromStr for RoomAddress {
    type Err = CoreError;

    fn from_str(input: &str) -> Result<Self, Self::Err> {
        let text = input.trim();
        if let Some(pin) = text.strip_prefix("南极") {
            if pin.len() != 4 || !pin.chars().all(|c| c.is_ascii_digit()) {
                return Err(CoreError::Address(
                    "南极 rooms need a four-digit PIN".into(),
                ));
            }
            return Self::new(Capital::Antarctica, pin, "1", "1");
        }
        let rest = text
            .strip_suffix('房')
            .ok_or_else(|| CoreError::Address("address must end with 房".into()))?;
        let (head, room) = rest
            .rsplit_once('级')
            .ok_or_else(|| CoreError::Address("address must contain 级".into()))?;
        let (head, level) = head
            .rsplit_once('区')
            .ok_or_else(|| CoreError::Address("address must contain 区".into()))?;
        let (capital, district) = Capital::ALL
            .into_iter()
            .find_map(|capital| {
                head.strip_prefix(capital.as_str())
                    .map(|district| (capital, district))
            })
            .ok_or_else(|| CoreError::Address("address must start with 东京 or 西京".into()))?;
        Self::new(capital, district, level, room)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_spec_example() {
        let address: RoomAddress = "东京A区1B级103房".parse().unwrap();
        assert_eq!(address.capital, Capital::Tokyo);
        assert_eq!(address.district, "A");
        assert_eq!(address.level, "1B");
        assert_eq!(address.room, "103");
        assert_eq!(address.to_string(), "东京A区1B级103房");
    }

    #[test]
    fn parses_the_western_capital_and_keeps_siblings_peer() {
        let address: RoomAddress = "西京Z区B级103房".parse().unwrap();
        assert_eq!(address.capital, Capital::Kyoto);
        assert_eq!(address.segments(), ["西京", "Z", "B", "103"]);
    }

    #[test]
    fn parses_a_pin_only_southern_room() {
        let address: RoomAddress = "南极1234".parse().unwrap();
        assert_eq!(address.capital, Capital::Antarctica);
        assert_eq!(address.district, "1234");
        assert_eq!(address.to_string(), "南极1234");
        assert!("南极123".parse::<RoomAddress>().is_err());
    }

    #[test]
    fn rejects_malformed_addresses() {
        for bad in [
            "",
            "东京A区1B级",
            "东京A区103房",
            "东京A区1B级103",
            "京都A区1B级103房",
            "东京A区1B级103房房",
            "东京 区1B级103房",
        ] {
            assert!(bad.parse::<RoomAddress>().is_err(), "{bad:?} should fail");
        }
    }

    #[test]
    fn rejects_reserved_characters_in_parts() {
        assert!(RoomAddress::new(Capital::Tokyo, "A区", "1B", "103").is_err());
        assert!(RoomAddress::new(Capital::Tokyo, "A", "", "103").is_err());
    }

    #[test]
    fn parses_with_surrounding_whitespace() {
        let address: RoomAddress = "  东京A区1B级103房  ".parse().unwrap();
        assert_eq!(address.room, "103");
    }
}
