//! Small, authored courses; content, records and medals share a stable identity.
pub struct Course {
    pub id: &'static str,
    pub name: &'static str,
    pub challenge: &'static str,
    pub source: &'static str,
    pub gold_seconds: f32,
}
pub const COURSES: [Course; 4] = [
    Course {
        id: "aqueduct",
        name: "Aqueduct",
        challenge: "Warm-up / open stone lanes",
        source: crate::level::SOURCE,
        gold_seconds: 35.,
    },
    Course {
        id: "needlewalk",
        name: "Needlewalk",
        challenge: "Hard / thin rails + switchbacks",
        source: include_str!("../content/needlewalk.fxscene"),
        gold_seconds: 34.,
    },
    Course {
        id: "clockwork",
        name: "Clockwork",
        challenge: "Expert / moving sweepers",
        source: include_str!("../content/clockwork.fxscene"),
        gold_seconds: 40.,
    },
    Course {
        id: "frostline",
        name: "Frostline",
        challenge: "Expert / ice + steep descents",
        source: include_str!("../content/frostline.fxscene"),
        gold_seconds: 32.,
    },
];
impl Course {
    pub fn context(&self) -> u64 {
        context(self.source)
    }
    /// The best medal still reachable mid-run and the time it must beat.
    ///
    /// Mirrors [`Self::medal`]: a finish before the returned deadline with the
    /// same falls earns the returned medal.
    pub fn pace(&self, elapsed: f32, falls: u32) -> (&'static str, Option<f32>) {
        if falls == 0 && elapsed <= self.gold_seconds {
            ("GOLD", Some(self.gold_seconds))
        } else if falls <= 2 && elapsed <= self.gold_seconds * 1.5 {
            ("SILVER", Some(self.gold_seconds * 1.5))
        } else {
            ("BRONZE", None)
        }
    }
    pub fn medal(&self, elapsed: f32, falls: u32) -> &'static str {
        if falls == 0 && elapsed <= self.gold_seconds {
            "GOLD"
        } else if falls <= 2 && elapsed <= self.gold_seconds * 1.5 {
            "SILVER"
        } else {
            "BRONZE"
        }
    }
}

/// Records belong to both authored geometry and the movement rules used to run it.
pub fn context(source: &str) -> u64 {
    ferravex_engine::persistence::checksum(source.as_bytes()) ^ 0x46584d4152420003
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pace_always_names_the_medal_an_immediate_finish_would_earn() {
        let c = &COURSES[0];
        for falls in 0..5 {
            for tenth in 0..800 {
                let elapsed = tenth as f32 * 0.1;
                let (medal, deadline) = c.pace(elapsed, falls);
                assert_eq!(medal, c.medal(elapsed, falls), "{elapsed}s {falls} falls");
                assert_eq!(deadline.is_none(), medal == "BRONZE");
                assert!(deadline.is_none_or(|d| elapsed <= d));
            }
        }
    }
}
