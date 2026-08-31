use async_trait::async_trait;
use serde_json::{Map, Value};
use std::sync::Arc;

use super::{LLMMessage, LLMService, LlmResult, OllamaService, Tool};

/// A deterministic LLM facade for development flows.
///
/// The first request contains the initial user message (length 1), and the
/// next request contains that message, the first assistant response, and the
/// tool result (length 3). All later requests are delegated to Ollama.
pub struct MockLlmService {
    fallback: Arc<OllamaService>,
}

impl MockLlmService {
    pub fn new(fallback: Arc<OllamaService>) -> Self {
        Self { fallback }
    }
}

#[async_trait]
impl LLMService for MockLlmService {
    async fn execute_prompt(
        &self,
        prompt: &str,
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<String> {
        self.fallback.execute_prompt(prompt, context).await
    }

    async fn execute_prompt_with_tools(
        &self,
        messages: &[LLMMessage],
        tools: &[Box<dyn Tool>],
        context: Option<&Map<String, Value>>,
    ) -> LlmResult<Value> {
        println!("Length of messages");
        println!("{}", messages.len().to_string());
        match messages.len() {
            2 => Ok(serde_json::json!({
                "content": "",
                "tool_calls": [{
                    "id": "mock-load-skill",
                    "type": "function",
                    "function": {
                        "name": "load_skill",
                        "arguments": { "skillName": "crud_creation" }
                    }
                }]
            })),
            4 => Ok(serde_json::json!({
                "content": "",
                "tool_calls": [{
                    "id": "mock-sync-schema",
                    "type": "function",
                    "function": {
                        "name": "sync_schema",
                        "arguments": {
                            "resources": [{
                                "endpoint": "/customers",
                                "fields": [
                                    { "field": "name", "name": "name", "type": "string" },
                                    { "field": "code", "name": "code", "type": "string" },
                                    { "field": "email", "name": "email", "type": "string" },
                                    { "field": "phone", "name": "phone", "type": "string" }
                                ],
                                "name": "customers"
                            }]
                        }
                    }
                }]
            })),
            6 => Ok(serde_json::json!({
                "content": "",
                "tool_calls": [{
                    "id": "mock-write-controller",
                    "type": "function",
                    "function": {
                        "name": "write_file",
                        "arguments": {
                            "shell": "backend",
                            "path": "Controller/CustomerController.php",
                            "content": "<?php

namespace App\\Controller;

use App\\Entity\\Customer;
use App\\Resource\\ResourceController;
use Symfony\\Component\\Routing\\Attribute\\Route;

#[Route('/api/customers', name: 'customers.')]
final class CustomerController extends ResourceController
{
    protected function getEntityClass(): string
    {
        return Customer::class;
    }

    protected function getResourceName(): string
    {
        return 'customers';
    }
}"
                        }
                    }
                }]
            })),
            8 => Ok(serde_json::json!({
                "content": "",
                "tool_calls": [{
                    "id": "mock-write-page",
                    "type": "function",
                    "function": {
                        "name": "write_page",
                        "arguments": {
                            "route": "/customers",
                            "code": "'use client';

import ResourcePage from '@/components/resources/ResourcePage';
import customersResource from '@/resources/customers';

export default function CustomersPage() {
  return <ResourcePage resource={customersResource} />;
}"
                        }
                    }
                }]
            })),
            10 => Ok(serde_json::json!({
                "content": "",
                "tool_calls": [{
                    "id": "mock-write-menu",
                    "type": "function",
                    "function": {
                        "name": "write_menu",
                        "arguments": {
                            "name": "CRM.Customers",
                            "route": "/customers",
                            "icon": "PersonOutlined",
                            "after": "Dashboard"
                        }
                    }
                }]
            })),
            _ => Ok(serde_json::json!({
                "content": "Done",
                "tool_calls": []
            })),
        }
    }
}
