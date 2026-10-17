//! Which model a run is asked, by the kind of run (`ADR-0196`).
//!
//! Most work does not need the strongest model: a scheduled check, a run a new mail started and a sub-agent's bounded errand are often
//! well served by a cheaper one, and the owner's own questions keep the model they chose. The kind is read from how the run began
//! (durable lineage for sub-agents; objective framing for schedules/triggers). A resumed child keeps its role.

use jarvis_models::ModelId;

/// The kind of run, as far as the choice of model goes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RunRole {
    /// What the owner asked directly.
    Main,
    /// A run a model started with `jarvis.agent.delegate`.
    SubAgent,
    /// A scheduled task, or a run a mail, file or webhook trigger started.
    Background,
}

/// Uses durable child identity before schedule/trigger framing.
pub(crate) fn role_of(objective: &str, is_delegated: bool) -> RunRole {
    if let Some(original) = objective.strip_prefix(crate::resume::RESUME_NOTICE) {
        return role_of(original, is_delegated);
    }
    if is_delegated {
        RunRole::SubAgent
    } else if objective.starts_with(crate::schedule_service::UNATTENDED_NOTICE)
        || crate::trigger_service::parse_triggered(objective).is_some()
    {
        RunRole::Background
    } else {
        RunRole::Main
    }
}

/// The models asked for the kinds of run that have their own; the main model serves the rest.
#[derive(Clone, Debug, Default)]
pub(crate) struct ModelRoutes {
    subagent: Option<ModelId>,
    background: Option<ModelId>,
}

impl ModelRoutes {
    /// Routes sub-agent and background runs to their own models when given.
    pub(crate) const fn new(subagent: Option<ModelId>, background: Option<ModelId>) -> Self {
        Self {
            subagent,
            background,
        }
    }

    /// The model for `role`, or `main` when that kind has none of its own.
    pub(crate) fn model_for<'a>(&'a self, role: RunRole, main: &'a ModelId) -> &'a ModelId {
        match role {
            RunRole::Main => main,
            RunRole::SubAgent => self.subagent.as_ref().unwrap_or(main),
            RunRole::Background => self.background.as_ref().unwrap_or(main),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> ModelId {
        ModelId::new(name).unwrap_or_else(|_| panic!("{name} is a valid model id"))
    }

    /// **A run's kind comes from how it began, and a resumed run keeps it.**
    #[test]
    fn a_run_kind_is_read_from_its_framing_and_survives_a_resume() {
        let plain = "what is on my calendar today";
        let scheduled = format!(
            "{}check the weather",
            crate::schedule_service::UNATTENDED_NOTICE
        );
        let delegated = format!("{}find the invoice", crate::delegate::DELEGATED_NOTICE);
        assert_eq!(role_of(plain, false), RunRole::Main);
        assert_eq!(role_of(&scheduled, false), RunRole::Background);
        assert_eq!(role_of(&delegated, true), RunRole::SubAgent);
        assert_eq!(
            role_of(&delegated, false),
            RunRole::Main,
            "text cannot select a child model"
        );
        assert_eq!(role_of(plain, true), RunRole::SubAgent);
        assert_eq!(
            role_of(&format!("{}{plain}", crate::resume::RESUME_NOTICE), true),
            RunRole::SubAgent
        );
        for original in [plain, scheduled.as_str(), delegated.as_str()] {
            let resumed = format!("{}{original}", crate::resume::RESUME_NOTICE);
            assert_eq!(role_of(&resumed, false), role_of(original, false));
        }
    }

    /// **A kind with no model of its own, and the owner's own questions, use the main model.**
    #[test]
    fn a_kind_without_its_own_model_uses_the_main_one() {
        let main = id("strong");
        let routes = ModelRoutes::new(Some(id("cheap-sub")), None);
        assert_eq!(routes.model_for(RunRole::Main, &main).as_str(), "strong");
        assert_eq!(
            routes.model_for(RunRole::SubAgent, &main).as_str(),
            "cheap-sub"
        );
        assert_eq!(
            routes.model_for(RunRole::Background, &main).as_str(),
            "strong"
        );
        let both = ModelRoutes::new(Some(id("cheap-sub")), Some(id("cheap-bg")));
        assert_eq!(
            both.model_for(RunRole::Background, &main).as_str(),
            "cheap-bg"
        );
        assert_eq!(both.model_for(RunRole::Main, &main).as_str(), "strong");
    }
}
