use crate::config::{ProviderTarget, RouteConfig, RoutingStrategy};
use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GatewayRequest {
    pub prompt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct GatewayResponse {
    pub strategy: RoutingStrategy,
    pub selected: Vec<ProviderResult>,
    pub failures: Vec<ProviderFailure>,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProviderResult {
    pub provider: String,
    pub model: String,
    pub latency_ms: u64,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct ProviderFailure {
    pub provider: String,
    pub model: String,
    pub reason: String,
}

pub fn route_request(route: &RouteConfig, request: &GatewayRequest) -> Result<GatewayResponse> {
    if route.providers.is_empty() {
        bail!("route {} has no providers", route.id);
    }
    let mut providers = route.providers.clone();
    providers.sort_by_key(|provider| provider.priority);
    match route.strategy {
        RoutingStrategy::Fixed => fixed(route.strategy, &providers[0], request),
        RoutingStrategy::Fallback => fallback(route.strategy, &providers, request),
        RoutingStrategy::Race => race(route.strategy, &providers, request),
        RoutingStrategy::Broadcast => broadcast(route.strategy, &providers, request),
    }
}

fn fixed(
    strategy: RoutingStrategy,
    provider: &ProviderTarget,
    request: &GatewayRequest,
) -> Result<GatewayResponse> {
    match execute(provider, request) {
        Ok(result) => Ok(GatewayResponse {
            strategy,
            content: result.content.clone(),
            selected: vec![result],
            failures: vec![],
        }),
        Err(failure) => bail!(
            "fixed route failed for {} {}: {}",
            failure.provider,
            failure.model,
            failure.reason
        ),
    }
}

fn fallback(
    strategy: RoutingStrategy,
    providers: &[ProviderTarget],
    request: &GatewayRequest,
) -> Result<GatewayResponse> {
    let mut failures = vec![];
    for provider in providers {
        match execute(provider, request) {
            Ok(result) => {
                return Ok(GatewayResponse {
                    strategy,
                    content: result.content.clone(),
                    selected: vec![result],
                    failures,
                });
            }
            Err(failure) => failures.push(failure),
        }
    }
    bail!("fallback route exhausted {} providers", failures.len())
}

fn race(
    strategy: RoutingStrategy,
    providers: &[ProviderTarget],
    request: &GatewayRequest,
) -> Result<GatewayResponse> {
    let mut successes = vec![];
    let mut failures = vec![];
    for provider in providers {
        match execute(provider, request) {
            Ok(result) => successes.push(result),
            Err(failure) => failures.push(failure),
        }
    }
    successes.sort_by_key(|result| result.latency_ms);
    let Some(winner) = successes.into_iter().next() else {
        bail!("race route had no successful providers");
    };
    Ok(GatewayResponse {
        strategy,
        content: winner.content.clone(),
        selected: vec![winner],
        failures,
    })
}

fn broadcast(
    strategy: RoutingStrategy,
    providers: &[ProviderTarget],
    request: &GatewayRequest,
) -> Result<GatewayResponse> {
    let mut selected = vec![];
    let mut failures = vec![];
    for provider in providers {
        match execute(provider, request) {
            Ok(result) => selected.push(result),
            Err(failure) => failures.push(failure),
        }
    }
    if selected.is_empty() {
        bail!("broadcast route had no successful providers");
    }
    let content = selected
        .iter()
        .map(|result| result.content.as_str())
        .collect::<Vec<_>>()
        .join("\n---\n");
    Ok(GatewayResponse {
        strategy,
        selected,
        failures,
        content,
    })
}

fn execute(
    provider: &ProviderTarget,
    request: &GatewayRequest,
) -> std::result::Result<ProviderResult, ProviderFailure> {
    if provider.simulate.succeeds {
        Ok(ProviderResult {
            provider: provider.provider.clone(),
            model: provider.model.clone(),
            latency_ms: provider.simulate.latency_ms,
            content: format!("{} | prompt={}", provider.simulate.response, request.prompt),
        })
    } else {
        Err(ProviderFailure {
            provider: provider.provider.clone(),
            model: provider.model.clone(),
            reason: "simulated provider failure".to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{EndpointKind, ProviderTarget};

    fn p(name: &str, priority: u32, succeeds: bool, latency: u64) -> ProviderTarget {
        ProviderTarget::new(
            name,
            "model",
            EndpointKind::OpenAiCompatible,
            priority,
            succeeds,
            latency,
        )
    }

    #[test]
    fn fixed_uses_first_provider() {
        let route = RouteConfig {
            id: "r".to_string(),
            strategy: RoutingStrategy::Fixed,
            providers: vec![p("a", 1, true, 20), p("b", 2, true, 1)],
        };
        let response = route_request(
            &route,
            &GatewayRequest {
                prompt: "x".to_string(),
            },
        )
        .unwrap();
        assert_eq!(response.selected[0].provider, "a");
    }

    #[test]
    fn fallback_skips_failed_provider() {
        let route = RouteConfig {
            id: "r".to_string(),
            strategy: RoutingStrategy::Fallback,
            providers: vec![p("bad", 1, false, 20), p("good", 2, true, 30)],
        };
        let response = route_request(
            &route,
            &GatewayRequest {
                prompt: "x".to_string(),
            },
        )
        .unwrap();
        assert_eq!(response.selected[0].provider, "good");
        assert_eq!(response.failures.len(), 1);
    }

    #[test]
    fn race_selects_fastest_success() {
        let route = RouteConfig {
            id: "r".to_string(),
            strategy: RoutingStrategy::Race,
            providers: vec![p("slow", 1, true, 80), p("fast", 2, true, 10)],
        };
        let response = route_request(
            &route,
            &GatewayRequest {
                prompt: "x".to_string(),
            },
        )
        .unwrap();
        assert_eq!(response.selected[0].provider, "fast");
    }

    #[test]
    fn broadcast_returns_all_successes() {
        let route = RouteConfig {
            id: "r".to_string(),
            strategy: RoutingStrategy::Broadcast,
            providers: vec![
                p("a", 1, true, 20),
                p("b", 2, true, 10),
                p("bad", 3, false, 1),
            ],
        };
        let response = route_request(
            &route,
            &GatewayRequest {
                prompt: "x".to_string(),
            },
        )
        .unwrap();
        assert_eq!(response.selected.len(), 2);
        assert_eq!(response.failures.len(), 1);
    }
}
