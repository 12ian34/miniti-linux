//! Launch gate routing (PLAN.md §4): force-update → terms → onboarding → main.
//! Wired through the `launch_gate` command; the frontend renders the matching
//! screen before the main app.

use serde::Serialize;

/// Which screen the app should present at launch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    ForceUpdate,
    Terms,
    Onboarding,
    Main,
}

/// Inputs that determine the launch gate.
#[derive(Debug, Clone)]
pub struct GateInputs {
    pub current_version: String,
    /// Empty / unknown when the backend could not be reached — never forces.
    pub min_version: String,
    pub accepted_terms_version: Option<String>,
    pub current_terms_version: String,
    pub first_launch: bool,
}

/// Compare dotted numeric versions. Returns Ordering of `a` vs `b`.
pub fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |s: &str| -> Vec<u64> {
        s.split('.')
            .map(|p| p.trim().parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (va, vb) = (parse(a), parse(b));
    let len = va.len().max(vb.len());
    for i in 0..len {
        let x = va.get(i).copied().unwrap_or(0);
        let y = vb.get(i).copied().unwrap_or(0);
        match x.cmp(&y) {
            std::cmp::Ordering::Equal => continue,
            non_eq => return non_eq,
        }
    }
    std::cmp::Ordering::Equal
}

/// True when `min_version` is a well-formed dotted version we should honour.
fn is_version_like(s: &str) -> bool {
    !s.trim().is_empty()
        && s.split('.')
            .all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
}

/// Route to the first gate that applies, in priority order.
pub fn route(inputs: &GateInputs) -> Gate {
    if is_version_like(&inputs.min_version)
        && compare_versions(&inputs.min_version, &inputs.current_version)
            == std::cmp::Ordering::Greater
    {
        return Gate::ForceUpdate;
    }
    let terms_ok = inputs
        .accepted_terms_version
        .as_deref()
        .map(|v| compare_versions(v, &inputs.current_terms_version) != std::cmp::Ordering::Less)
        .unwrap_or(false);
    if !terms_ok {
        return Gate::Terms;
    }
    if inputs.first_launch {
        return Gate::Onboarding;
    }
    Gate::Main
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cmp::Ordering;

    #[test]
    fn version_comparison() {
        assert_eq!(compare_versions("1.2.0", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("1.3.0", "1.2.9"), Ordering::Greater);
        assert_eq!(compare_versions("1.2", "1.2.0"), Ordering::Equal);
        assert_eq!(compare_versions("0.9.0", "1.0.0"), Ordering::Less);
    }

    #[test]
    fn force_update_wins() {
        let g = route(&GateInputs {
            current_version: "0.1.0".into(),
            min_version: "1.0.0".into(),
            accepted_terms_version: Some("1.0".into()),
            current_terms_version: "1.0".into(),
            first_launch: false,
        });
        assert_eq!(g, Gate::ForceUpdate);
    }

    #[test]
    fn unknown_min_version_never_forces() {
        for min in ["", "unknown", "1.x"] {
            let g = route(&GateInputs {
                current_version: "0.1.0".into(),
                min_version: min.into(),
                accepted_terms_version: Some("1.0".into()),
                current_terms_version: "1.0".into(),
                first_launch: false,
            });
            assert_eq!(g, Gate::Main, "min_version {min:?} must not force");
        }
    }

    #[test]
    fn terms_then_onboarding_then_main() {
        let base = GateInputs {
            current_version: "1.0.0".into(),
            min_version: "1.0.0".into(),
            accepted_terms_version: None,
            current_terms_version: "2.0".into(),
            first_launch: true,
        };
        assert_eq!(route(&base), Gate::Terms);

        let accepted = GateInputs {
            accepted_terms_version: Some("2.0".into()),
            ..base.clone()
        };
        assert_eq!(route(&accepted), Gate::Onboarding);

        let returning = GateInputs {
            first_launch: false,
            ..accepted
        };
        assert_eq!(route(&returning), Gate::Main);
    }

    #[test]
    fn stale_accepted_terms_reprompts() {
        let g = route(&GateInputs {
            current_version: "1.0.0".into(),
            min_version: "1.0.0".into(),
            accepted_terms_version: Some("1.0".into()),
            current_terms_version: "2.0".into(),
            first_launch: false,
        });
        assert_eq!(g, Gate::Terms);
    }

    #[test]
    fn gate_serializes_snake_case() {
        assert_eq!(
            serde_json::to_value(Gate::ForceUpdate).unwrap(),
            "force_update"
        );
    }
}
