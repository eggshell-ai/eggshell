//! The ordered catalogue of checks.
//!
//! Registering a check here is the only wiring a new check needs — `lint_code`
//! discovers everything from this list. Cheap static checks come first so the
//! agent gets the most actionable feedback even if a slower check fails.

use super::check::{Check, Tier};
use super::frontend::{EslintCheck, MenuJsonCheck};
use super::php::{AnalyticsReportsCheck, DoctrineSchemaCheck, PhpSyntaxCheck};
use super::resources::ResourceActionsCheck;
use super::symfony_routing::{DeprecatedRouteImportCheck, RouteRegistrationCheck};

/// Every check, in execution order.
pub(crate) fn default_checks() -> Vec<Box<dyn Check>> {
    let mut checks: Vec<Box<dyn Check>> = vec![
        Box::new(DeprecatedRouteImportCheck),
        Box::new(MenuJsonCheck),
        Box::new(ResourceActionsCheck),
        Box::new(PhpSyntaxCheck),
        Box::new(EslintCheck),
        Box::new(RouteRegistrationCheck),
        Box::new(DoctrineSchemaCheck),
        Box::new(AnalyticsReportsCheck),
    ];

    checks.sort_by_key(|check| check.tier());
    checks
}

/// Human-readable catalogue used by the tool description, stable ordering.
pub(crate) fn describe_checks(checks: &[Box<dyn Check>]) -> String {
    checks
        .iter()
        .map(|check| {
            let cost = match check.tier() {
                Tier::Static => "static",
                Tier::Command => {
                    if check.requires_deep() {
                        "deep"
                    } else {
                        "command"
                    }
                }
            };
            format!("'{}' ({cost}) - {}", check.id(), check.description())
        })
        .collect::<Vec<_>>()
        .join("; ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_ids_are_unique() {
        let checks = default_checks();
        let mut ids: Vec<&str> = checks.iter().map(|check| check.id()).collect();
        let total = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), total, "check ids must be unique");
    }

    #[test]
    fn static_checks_run_before_command_checks() {
        let checks = default_checks();
        let first_command = checks.iter().position(|c| c.tier() == Tier::Command);
        let last_static = checks.iter().rposition(|c| c.tier() == Tier::Static);

        if let (Some(first_command), Some(last_static)) = (first_command, last_static) {
            assert!(last_static < first_command);
        }
    }

    #[test]
    fn description_lists_every_check() {
        let checks = default_checks();
        let described = describe_checks(&checks);
        for check in &checks {
            assert!(described.contains(check.id()));
        }
    }
}
