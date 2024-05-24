use lazy_regex::regex;
use std::fmt;
use std::str::FromStr;

#[derive(Default, Debug, Ord, PartialOrd, Eq, PartialEq, Clone, Hash)]
pub struct ControlID {
    subject: String,
    number: u8,
    subnumber: u8,
}

impl ControlID {
    pub fn is_empty(&self) -> bool {
        return self.subject.is_empty() || self.number == 0;
    }
}

impl fmt::Display for ControlID {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.subnumber > 0 {
            write!(f, "{}-{} ({})", self.subject, self.number, self.subnumber)
        } else {
            write!(f, "{}-{}", self.subject, self.number)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct ParseControlIDErr;

impl FromStr for ControlID {
    type Err = ParseControlIDErr;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let rx = regex!(r"(?<subject>\w+)-(?<number>\d+)(?:\s+\((?<subnumber>\d+)\))?");
        let caps = rx.captures(s).ok_or(ParseControlIDErr)?;
        let subject = caps.name("subject").ok_or(ParseControlIDErr)?.as_str();
        let number = caps
            .name("number")
            .map(|n| n.as_str().parse::<u8>().unwrap())
            .ok_or(ParseControlIDErr)?;
        let subnumber = caps
            .name("subnumber")
            .map(|n| n.as_str().parse::<u8>().unwrap())
            .unwrap_or_default();
        return Ok(ControlID {
            subject: subject.to_string(),
            number,
            subnumber,
        });
    }
}
