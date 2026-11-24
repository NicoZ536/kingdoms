//! Player policies: settings that steer the simulation without being entities (tax rate, job
//! priorities). Changed only through [`Command`](crate::command::Command)s.

use serde::{Deserialize, Serialize};

use crate::defs::JobCategory;

/// Priority level (docs/game_design.md §6 and §7: niedrig / normal / hoch).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JobPriority {
    /// Filled last.
    Low,
    /// Default.
    #[default]
    Normal,
    /// Filled first.
    High,
}

impl JobPriority {
    /// German display name.
    pub const fn display_name(self) -> &'static str {
        match self {
            JobPriority::Low => "Niedrig",
            JobPriority::Normal => "Normal",
            JobPriority::High => "Hoch",
        }
    }
}

/// Current player policies.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Policy {
    /// Tax rate in percent (0 ..= `GameRules::max_tax_rate_percent`).
    pub tax_rate_percent: u8,
    /// Priority per job category, indexed by [`JobCategory::index`].
    job_priorities: [JobPriority; JobCategory::ALL.len()],
}

impl Policy {
    /// Policy with the given tax rate and all job priorities at [`JobPriority::Normal`].
    pub fn new(tax_rate_percent: u8) -> Self {
        Self { tax_rate_percent, job_priorities: [JobPriority::Normal; JobCategory::ALL.len()] }
    }

    /// Priority of a job category.
    pub fn job_priority(&self, category: JobCategory) -> JobPriority {
        self.job_priorities[category.index()]
    }

    /// Sets the priority of a job category; returns true if it changed.
    pub fn set_job_priority(&mut self, category: JobCategory, priority: JobPriority) -> bool {
        let slot = &mut self.job_priorities[category.index()];
        let changed = *slot != priority;
        *slot = priority;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_priorities_default_to_normal_and_change() {
        let mut p = Policy::new(10);
        assert!(JobCategory::ALL.iter().all(|c| p.job_priority(*c) == JobPriority::Normal));
        assert!(p.set_job_priority(JobCategory::Food, JobPriority::High));
        assert!(!p.set_job_priority(JobCategory::Food, JobPriority::High));
        assert_eq!(p.job_priority(JobCategory::Food), JobPriority::High);
        assert_eq!(p.job_priority(JobCategory::Military), JobPriority::Normal);
    }
}
