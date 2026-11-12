# About Langfuse

This page explains what Langfuse is and the two ways to run it: **Langfuse Cloud** (managed by Langfuse) and **Langfuse open source** (self-hosted). It was written from the official Langfuse documentation, retrieved through the Langfuse Docs MCP server on 2026-11-12. Prices, limits and adoption figures change, so check the linked pages under [Sources](#sources) before relying on them.

## What is Langfuse?

Langfuse is an open-source AI engineering platform. Teams use it to debug, analyze and improve applications built on large language models (LLMs) and AI agents. Its features share one data model, so a prompt version, the traces it produced and the scores those traces received are linked to each other.

| Area | What it does |
| --- | --- |
| [Observability](https://langfuse.com/docs/observability/overview) | Records a trace for each request: every LLM call, retrieval, tool call and other step, with inputs, outputs, latency, token usage and cost. Traces group into sessions (conversations) and users, and agent runs can be shown as graphs. |
| [Prompt management](https://langfuse.com/docs/prompt-management/overview) | Stores prompts with versions and labels such as `production`, so a prompt can be changed and deployed without a code release. Includes a playground for testing prompts. Skill management (beta) does the same for agent instructions and their files. |
| [Evaluation](https://langfuse.com/docs/evaluation/overview) | Attaches scores to traces from LLM-as-a-judge evaluators, code evaluators, user feedback, human annotation queues or your own pipeline. Datasets and experiments test a change before it ships. |
| [Metrics](https://langfuse.com/docs/metrics/overview) | Dashboards for quality, cost and latency, plus alerts when a metric crosses a threshold. |
| [AI Gateway](https://langfuse.com/docs/gateway/overview) (beta) | One endpoint for calls to model providers. It keeps provider credentials in one place and records each call as a trace. It is optional. |
| [API and data platform](https://langfuse.com/docs/api-and-data-platform/overview) | A public REST API, exports, a CLI and an MCP server for reading and writing the same data programmatically. |

Data reaches Langfuse through the Python and JavaScript/TypeScript SDKs, through integrations with frameworks and model providers, through OpenTelemetry, or through a gateway. Tracing is based on OpenTelemetry, so any language that can export OTLP can send traces.

Langfuse describes the intended workflow as a loop. In production you trace and monitor the application. In development you build datasets from what monitoring found, run experiments and evaluate the results. You deploy the change, and new traces start the loop again.

## Two ways to run it

Langfuse Cloud and self-hosted Langfuse run the same codebase, and the SDKs and integrations work the same way against both. The difference is who operates the infrastructure and where the data is stored.

| | Langfuse Cloud | Langfuse open source (self-hosted) |
| --- | --- | --- |
| Operated by | The Langfuse team | You |
| Data location | A Langfuse region on AWS that you choose | Your own infrastructure, in any region or on-premises |
| Cost | Free Hobby plan, then a monthly fee plus usage | Free under the MIT license; you pay for your infrastructure |
| Usage limits | Set by the plan | None |
| Setup | Sign up and create API keys | Deploy two containers and four data stores |

## Langfuse Cloud

[Langfuse Cloud](https://cloud.langfuse.com) is the fully managed, multi-tenant service. It needs no infrastructure: you sign up, create a project and use its API keys.

### Regions

Each region is a separate environment. Data, user accounts and infrastructure are not shared between regions, so moving to another region means creating a new account and migrating the data.

| Region | URL | Location |
| --- | --- | --- |
| EU | `https://cloud.langfuse.com` | Ireland (AWS `eu-west-1`) |
| US | `https://us.cloud.langfuse.com` | Oregon (AWS `us-west-2`) |
| Japan | `https://jp.cloud.langfuse.com` | Tokyo (AWS `ap-northeast-1`) |
| HIPAA | `https://hipaa.cloud.langfuse.com` | Oregon (AWS `us-west-2`) |

### Plans

Usage is counted in billable units. A unit is one tracing data point: a trace, an observation (span, event or generation) or a score.

| Plan | Price | Included units per month | Data access | Users |
| --- | --- | --- | --- | --- |
| Hobby | Free, no credit card | 50k | 30 days | 2 |
| Core | $29/month | 100k | 90 days | Unlimited |
| Pro | $199/month | 100k | 3 years | Unlimited |
| Enterprise | $2,499/month | 100k | 3 years | Unlimited |

- Paid plans bill additional units at $8 per 100k, with lower rates at higher volumes (down to $6 per 100k above 50M units).
- The Hobby plan has all platform features, with limits such as one annotation queue and lower API rate limits.
- Pro adds data retention management, higher rate limits, SOC 2 and ISO 27001 reports, and a BAA for HIPAA. The Teams add-on ($300/month) adds enterprise SSO, SSO enforcement and project-level RBAC.
- Enterprise adds audit logs, the SCIM API, custom rate limits, and uptime and support SLAs.
- Data access is how far back you can view data on a plan. Older data stays stored and becomes visible again after an upgrade.

### Security and operations

- Langfuse Cloud runs on AWS and ClickHouse Cloud, with encryption at rest and in transit.
- It is covered by SOC 2 Type II and ISO 27001 audits and annual third-party penetration tests. It is GDPR compliant, and a data processing agreement is available on every plan.
- Every record belongs to a project, API keys are project-scoped, and requests are authorized through role-based access control (RBAC).
- Databases run across multiple availability zones. The stated recovery objectives are an RPO of 10 minutes for Postgres and an RTO of 12 hours.
- Postgres backups are copied to a second AWS region in the same jurisdiction. Tracing data in ClickHouse and uploaded media are not, so Langfuse recommends the scheduled export to your own blob storage if you need protection against the loss of a region.

## Langfuse open source

Langfuse is developed in the open at [github.com/langfuse/langfuse](https://github.com/langfuse/langfuse). The core is MIT-licensed, and you can run it on your own infrastructure for free, with no usage limits.

### What the open-source version includes

All core platform features and APIs are included: tracing, sessions and users, token and cost tracking, prompt management and the playground, datasets and experiments, LLM-as-a-judge evaluators, annotation queues, custom dashboards, the AI Gateway, the public API, and exports. It also includes sign-in with Google, Azure AD and GitHub, enterprise SSO with enforcement, and organization-level RBAC. Projects and users are unlimited. Support comes from the community on GitHub.

### What needs a license key

Some add-on features require a paid [license key](https://langfuse.com/self-hosting/license-key):

- **Self-Hosted Enterprise** adds project-level RBAC, data retention management, audit logs, the SCIM and admin APIs, server-side data masking, protected prompt labels, UI customization, and the instance management API. It also comes with a support SLA, a named support engineer and the SOC 2 and ISO 27001 reports. It is bundled with a ClickHouse commercial plan.
- **Self-Hosted Government** adds FIPS 140-3 validated cryptographic modules, Iron Bank-hardened base images and air-gapped deployment.

### Architecture

A self-hosted deployment runs the same components as Langfuse Cloud, and all of them are open source:

| Component | Purpose |
| --- | --- |
| Langfuse Web (`langfuse/langfuse`) | Serves the UI and the APIs |
| Langfuse Worker (`langfuse/worker`) | Processes events asynchronously |
| PostgreSQL | Transactional data: organizations, projects, users, API keys, prompts |
| ClickHouse | Observability data: traces, observations and scores |
| Redis or Valkey | Cache and job queue |
| S3 or other blob storage | Raw events, multi-modal attachments and large exports |

Incoming events are written to blob storage first and to the database only after processing, so a database outage delays events and does not lose them. An LLM API is optional: the playground and the managed evaluators use one if you connect it. The AI Gateway (`langfuse/ai-gateway`, beta) is a third, optional container. Internet access is optional, and the deployment can run inside a VPC or on-premises.

### Deployment options

| Use | Option |
| --- | --- |
| Local use and testing | [Docker Compose](https://langfuse.com/self-hosting/deployment/docker-compose), on a single machine without high availability, scaling or backups |
| Production | [Kubernetes with Helm](https://langfuse.com/self-hosting/deployment/kubernetes-helm), or Terraform modules for [AWS](https://langfuse.com/self-hosting/deployment/aws), [Azure](https://langfuse.com/self-hosting/deployment/azure) and [GCP](https://langfuse.com/self-hosting/deployment/gcp) |
| Community-supported | [Render](https://langfuse.com/self-hosting/deployment/render) and [Railway](https://langfuse.com/self-hosting/deployment/railway) |

When you self-host, you are responsible for upgrades, scaling, backups and security of the deployment. Releases are tagged with semantic versions and published on GitHub.

## Which one to choose

- Choose **Langfuse Cloud** to start quickly or to avoid operating databases. The free Hobby plan is enough for a proof of concept.
- Choose **self-hosted Langfuse** when data must stay in your own account, VPC or data center, when you need a region Langfuse Cloud does not offer, or when you want no usage-based fees.
- You can switch later. The APIs are the same, and Langfuse publishes a [data migration cookbook](https://langfuse.com/guides/cookbook/example_data_migration) for moving traces, prompts and datasets between projects.

## Using this SDK with either

The Rust SDK in this repository talks to the Langfuse public API, so it works with Langfuse Cloud and with a self-hosted instance. Only the base URL differs:

```sh
# Langfuse Cloud, EU region (the SDK default)
LANGFUSE_BASE_URL=https://cloud.langfuse.com

# Langfuse Cloud, US region
LANGFUSE_BASE_URL=https://us.cloud.langfuse.com

# Self-hosted
LANGFUSE_BASE_URL=https://langfuse.example.com
```

`LANGFUSE_PUBLIC_KEY` and `LANGFUSE_SECRET_KEY` come from the project settings of that instance. Keys are tied to one project in one region or instance.

## Sources

- [Langfuse overview](https://langfuse.com/docs)
- [Cloud pricing](https://langfuse.com/pricing)
- [Data regions and availability](https://langfuse.com/security/data-regions)
- [Security and compliance](https://langfuse.com/security)
- [Self-hosting](https://langfuse.com/self-hosting)
- [Self-hosted pricing and license](https://langfuse.com/pricing-self-host)
- [Documentation index (`llms.txt`)](https://langfuse.com/llms.txt)
