use std::collections::HashSet;
use pubgrub::version_set::VersionSet;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use smallvec::SmallVec;
use std::fmt::{Debug, Display, Formatter};

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
enum Part {
    Wildcard,
    Literal(HashSet<String>),
    InverseLiteral(HashSet<String>),
}

#[derive(Clone, Eq, PartialEq, Hash, Debug)]
struct GlobString {
    parts: SmallVec<[Part; 3]>,
}

impl<T: AsRef<str>> From<T> for GlobString {
    fn from(str: T) -> Self {
        let mut parts = SmallVec::new();
        let mut str = str.as_ref();
        while let Some((before, rest)) = str.split_once('*') {
            if before.len() > 0 {
                parts.push(Part::Literal(HashSet::from([String::from(before)])));
            }
            if !matches!(parts.last(), Some(Part::Wildcard)) {
                parts.push(Part::Wildcard);
            }
            str = rest;
        }
        if str.len() > 0 {
            parts.push(Part::Literal(HashSet::from([String::from(str)])));
        }
        Self { parts }
    }
}

impl Display for GlobString {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for part in self.parts.iter() {
            match part {
                Part::Wildcard => write!(f, "*")?,
                Part::Literal(lit) => {
                    if lit.len() > 1 {
                        write!(f, "(")?;
                    }
                    for (i, lit) in lit.iter().enumerate() {
                        if i > 0 {
                            write!(f, "|")?;
                        }
                        write!(f, "{}", lit)?;
                    }
                    if lit.len() > 1 {
                        write!(f, ")")?;
                    }
                },
                Part::InverseLiteral(lit) => {
                    write!(f, "!")?;
                    if lit.len() > 1 {
                        write!(f, "(")?;
                    }
                    for (i, lit) in lit.iter().enumerate() {
                        if i > 0 {
                            write!(f, "|")?;
                        }
                        write!(f, "{}", lit)?;
                    }
                    if lit.len() > 1 {
                        write!(f, ")")?;
                    }
                },
            }
        }
        Ok(())
    }
}

impl Serialize for GlobString {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{}", self))
    }
}

impl<'de> Deserialize<'de> for GlobString {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let str = String::deserialize(deserializer)?;
        Ok(Self::from(str))
    }
}

impl VersionSet for GlobString {
    type V = String;

    fn empty() -> Self {
        Self {
            parts: SmallVec::new(),
        }
    }

    fn full() -> Self {
        Self {
            parts: smallvec::smallvec![Part::Wildcard],
        }
    }

    fn singleton(v: Self::V) -> Self {
        Self {
            parts: smallvec::smallvec![Part::Literal(v)],
        }
    }

    fn complement(&self) -> Self {
        if self == &Self::empty() {
            Self::full()
        } else if self == &Self::full() {
            Self::empty()
        } else {
            Self {
                parts: self
                    .parts
                    .iter()
                    .filter_map(|part| match part {
                        Part::Wildcard => Some(Part::Wildcard),
                        Part::Literal(lit) => Some(Part::InverseLiteral(lit.clone())),
                        Part::InverseLiteral(lit) => Some(Part::Literal(lit.clone())),
                    })
                    .collect(),
            }
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        let mut parts = SmallVec::new();
        let mut left_iter = other.parts.iter();
        let mut right_iter = other.parts.iter();
        let mut left = left_iter.next();
        let mut right = right_iter.next();
        loop {
            match (left, right) {
                (None, _)|(_, None) => break,
                (Some(Part::Wildcard), Some(Part::Wildcard)) => {
                    if !matches(parts.last(), Some(Part::Wildcard)) {
                        parts.push(Part::Wildcard)
                    }
                    left = left_iter.next();
                    right = right_iter.next();
                }
                (Some(Part::Literal(a)), Some(Part::Literal(b))) => {
                    let intersection: HashSet<_> = a.intersection(b).cloned().collect();
                    if intersection.is_empty() {
                        return Self::empty()
                    } else{
                        parts.push(Part::Literal(intersection))
                    }
                    left = left_iter.next();
                    right = right_iter.next();
                }
                (Some(Part::Literal(a)), Some(Part::Wildcard))|(Some(Part::Wildcard), Some(Part::Literal(a))) => {
                    parts.push(Part::Literal(a.clone()));
                    left = left_iter.next();
                    right = right_iter.next();
                }

                (Some(Part::InverseLiteral(a)), Some(Part::InverseLiteral(b))) => {
                    parts.push(Part::InverseLiteral(a.union(b).cloned().collect()));
                    left = left_iter.next();
                    right = right_iter.next();
                }
                (Some(Part::InverseLiteral(a)), Some(Part::Wildcard))|(Some(Part::Wildcard), Some(Part::InverseLiteral(a))) => {
                    parts.push(Part::InverseLiteral(a.clone()));
                    left = left_iter.next();
                    right = right_iter.next();
                }
            }
        }

         Self {
             parts
         }
    }

    fn contains(&self, v: &Self::V) -> bool {
        return matches(self.parts.as_slice(), v.as_str());
        fn matches(parts: &[Part], str: &str) -> bool {
            if parts.is_empty() {
                str.is_empty()
            } else if str.is_empty() {
                let mut parts = parts;
                while matches!(parts.first(), Some(Part::Wildcard)) {
                    parts = &parts[1..];
                }
                parts.is_empty()
            } else {
                match &parts[0] {
                    Part::Wildcard => matches(&parts[1..], str) || matches(parts, &str[1..]),
                    Part::Literal(lit) => {
                        if let Some(rest) = str.strip_prefix(lit) {
                            matches(&parts[1..], rest)
                        } else {
                            false
                        }
                    }
                    Part::InverseLiteral(lit) => {
                        if str.starts_with(lit) {
                            false
                        } else {
                            matches(&parts[1..], str)
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use super::GlobString;
    use itertools::Itertools;
    use proptest::collection;
    use proptest::proptest;
    use pubgrub::version_set::VersionSet;

    proptest! {
      #[test]
      fn test_parse_identity(strings in collection::vec("[a-zA-Z0-9_-]{1,5}", 0..10)) {
        let str:String = strings.join("*");
        let glob_str = GlobString::from(&str);
        println!("{}", str);
        assert_eq!(str, format!("{}", glob_str));
      }

      #[test]
      fn test_complement(strings in collection::vec("[a-zA-Z0-9_-]{0,5}", 0..10)) {
        let str:String = strings.join("*");
        let glob_str = GlobString::from(&str);
        assert_eq!(glob_str.complement().complement(), glob_str);
      }

      #[test]
      fn test_contains(strings in collection::vec("[a-zA-Z0-9_-]{0,5}", 0..10)) {
        let glob_str = strings.iter().enumerate().map(|(i, str)| if (i % 2) == 0 { str.to_owned() } else { String::from("*")} ).join("");
        let test_str = strings.into_iter().join("");
        let glob_str = GlobString::from(&glob_str);
        println!("'{}' contains '{}'", glob_str, test_str);
        assert!(glob_str.contains(&test_str));
      }
    }

    #[test]
    fn edge_cases_contains() {
        assert!(GlobString::from("foobar").contains(&String::from("foobar")));
        assert!(GlobString::from("*").contains(&String::from("foobar")));
        assert!(GlobString::from("*bar").contains(&String::from("foobar")));
        assert!(GlobString::from("foo*").contains(&String::from("foobar")));
        assert!(GlobString::from("f*r").contains(&String::from("foobar")));
        assert!(!GlobString::from("").contains(&String::from("barfoo")));
        assert!(!GlobString::from("*bar").contains(&String::from("barfoo")));
        assert!(!GlobString::from("foo*").contains(&String::from("barfoo")));
        assert!(!GlobString::from("foobar").contains(&String::from("barfoo")));
    }
}
