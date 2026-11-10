//! Blocking wrappers of the API clients (generated from `src/api/*.rs`;
//! the `blocking_tests` test checks that none is missing).

/// Blocking clients of [`crate::api::annotation_queues`].
pub mod annotation_queues {
    #![allow(unused_imports)]
    pub use crate::api::annotation_queues::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::annotation_queues::{
        AnnotationQueue, AnnotationQueueAssignmentRequest, AnnotationQueueItem,
        AnnotationQueueStatus, CreateAnnotationQueueAssignmentResponse,
        CreateAnnotationQueueItemRequest, CreateAnnotationQueueRequest,
        DeleteAnnotationQueueAssignmentResponse, DeleteAnnotationQueueItemResponse,
        UpdateAnnotationQueueItemRequest,
    };
    use crate::pagination::{PageParams, Pages, Paginated, impl_page_query, paginate_pages};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`AnnotationQueuesClient`](crate::api::annotation_queues::AnnotationQueuesClient).
    #[derive(Clone)]
    pub struct AnnotationQueuesClient {
        pub(crate) inner: crate::api::annotation_queues::AnnotationQueuesClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for AnnotationQueuesClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("AnnotationQueuesClient")
                .finish_non_exhaustive()
        }
    }

    impl AnnotationQueuesClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list_queues`](crate::api::annotation_queues::AnnotationQueuesClient::list_queues).
            fn list_queues(&self, params: &PageParams) -> Result<Paginated<AnnotationQueue>, LangfuseError>;
            /// Blocking [`create_queue`](crate::api::annotation_queues::AnnotationQueuesClient::create_queue).
            fn create_queue(&self, req: &CreateAnnotationQueueRequest) -> Result<AnnotationQueue, LangfuseError>;
            /// Blocking [`get_queue`](crate::api::annotation_queues::AnnotationQueuesClient::get_queue).
            fn get_queue(&self, queue_id: &str) -> Result<AnnotationQueue, LangfuseError>;
            /// Blocking [`list_items`](crate::api::annotation_queues::AnnotationQueuesClient::list_items).
            fn list_items(&self, queue_id: &str, params: &ListQueueItemsParams) -> Result<Paginated<AnnotationQueueItem>, LangfuseError>;
            /// Blocking [`create_item`](crate::api::annotation_queues::AnnotationQueuesClient::create_item).
            fn create_item(&self, queue_id: &str, req: &CreateAnnotationQueueItemRequest) -> Result<AnnotationQueueItem, LangfuseError>;
            /// Blocking [`get_item`](crate::api::annotation_queues::AnnotationQueuesClient::get_item).
            fn get_item(&self, queue_id: &str, item_id: &str) -> Result<AnnotationQueueItem, LangfuseError>;
            /// Blocking [`update_item`](crate::api::annotation_queues::AnnotationQueuesClient::update_item).
            fn update_item(&self, queue_id: &str, item_id: &str, req: &UpdateAnnotationQueueItemRequest) -> Result<AnnotationQueueItem, LangfuseError>;
            /// Blocking [`delete_item`](crate::api::annotation_queues::AnnotationQueuesClient::delete_item).
            fn delete_item(&self, queue_id: &str, item_id: &str) -> Result<DeleteAnnotationQueueItemResponse, LangfuseError>;
            /// Blocking [`create_assignment`](crate::api::annotation_queues::AnnotationQueuesClient::create_assignment).
            fn create_assignment(&self, queue_id: &str, user_id: &str) -> Result<CreateAnnotationQueueAssignmentResponse, LangfuseError>;
            /// Blocking [`delete_assignment`](crate::api::annotation_queues::AnnotationQueuesClient::delete_assignment).
            fn delete_assignment(&self, queue_id: &str, user_id: &str) -> Result<DeleteAnnotationQueueAssignmentResponse, LangfuseError>;
        }

        /// Blocking [`list_queues_pages`](crate::api::annotation_queues::AnnotationQueuesClient::list_queues_pages): an iterator over the pages.
        pub fn list_queues_pages(
            &self,
            params: PageParams,
        ) -> BlockingPages<'_, Paginated<AnnotationQueue>> {
            BlockingPages::new(self.inner.list_queues_pages(params), &self.runtime)
        }

        /// Blocking [`list_items_pages`](crate::api::annotation_queues::AnnotationQueuesClient::list_items_pages): an iterator over the pages.
        pub fn list_items_pages<'a>(
            &'a self,
            queue_id: &'a str,
            params: ListQueueItemsParams,
        ) -> BlockingPages<'a, Paginated<AnnotationQueueItem>> {
            BlockingPages::new(self.inner.list_items_pages(queue_id, params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::blob_storage_integrations`].
pub mod blob_storage_integrations {
    #![allow(unused_imports)]
    pub use crate::api::blob_storage_integrations::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::blob_storage_integrations::{
        BlobStorageIntegrationDeletionResponse, BlobStorageIntegrationResponse,
        BlobStorageIntegrationStatusResponse, BlobStorageIntegrationsResponse,
        CreateBlobStorageIntegrationRequest,
    };
    use std::sync::Arc;

    /// Blocking version of [`BlobStorageIntegrationsClient`](crate::api::blob_storage_integrations::BlobStorageIntegrationsClient).
    #[derive(Clone)]
    pub struct BlobStorageIntegrationsClient {
        pub(crate) inner: crate::api::blob_storage_integrations::BlobStorageIntegrationsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for BlobStorageIntegrationsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BlobStorageIntegrationsClient")
                .finish_non_exhaustive()
        }
    }

    impl BlobStorageIntegrationsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::blob_storage_integrations::BlobStorageIntegrationsClient::list).
            fn list(&self) -> Result<BlobStorageIntegrationsResponse, LangfuseError>;
            /// Blocking [`upsert`](crate::api::blob_storage_integrations::BlobStorageIntegrationsClient::upsert).
            fn upsert(&self, req: &CreateBlobStorageIntegrationRequest) -> Result<BlobStorageIntegrationResponse, LangfuseError>;
            /// Blocking [`get_status`](crate::api::blob_storage_integrations::BlobStorageIntegrationsClient::get_status).
            fn get_status(&self, id: &str) -> Result<BlobStorageIntegrationStatusResponse, LangfuseError>;
            /// Blocking [`delete`](crate::api::blob_storage_integrations::BlobStorageIntegrationsClient::delete).
            fn delete(&self, id: &str) -> Result<BlobStorageIntegrationDeletionResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::comments`].
pub mod comments {
    #![allow(unused_imports)]
    pub use crate::api::comments::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::comments::{Comment, CreateCommentRequest, CreateCommentResponse};
    use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`CommentsClient`](crate::api::comments::CommentsClient).
    #[derive(Clone)]
    pub struct CommentsClient {
        pub(crate) inner: crate::api::comments::CommentsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for CommentsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("CommentsClient").finish_non_exhaustive()
        }
    }

    impl CommentsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`create`](crate::api::comments::CommentsClient::create).
            fn create(&self, req: &CreateCommentRequest) -> Result<CreateCommentResponse, LangfuseError>;
            /// Blocking [`list`](crate::api::comments::CommentsClient::list).
            fn list(&self, params: &ListCommentsParams) -> Result<Paginated<Comment>, LangfuseError>;
            /// Blocking [`get`](crate::api::comments::CommentsClient::get).
            fn get(&self, comment_id: &str) -> Result<Comment, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::comments::CommentsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListCommentsParams,
        ) -> BlockingPages<'_, Paginated<Comment>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::dataset_items`].
pub mod dataset_items {
    #![allow(unused_imports)]
    pub use crate::api::dataset_items::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::dataset_items::{
        CreateDatasetItemRequest, DatasetItem, DeleteDatasetItemResponse,
    };
    use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`DatasetItemsClient`](crate::api::dataset_items::DatasetItemsClient).
    #[derive(Clone)]
    pub struct DatasetItemsClient {
        pub(crate) inner: crate::api::dataset_items::DatasetItemsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for DatasetItemsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("DatasetItemsClient").finish_non_exhaustive()
        }
    }

    impl DatasetItemsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::dataset_items::DatasetItemsClient::list).
            fn list(&self, params: &ListDatasetItemsParams) -> Result<Paginated<DatasetItem>, LangfuseError>;
            /// Blocking [`create`](crate::api::dataset_items::DatasetItemsClient::create).
            fn create(&self, req: &CreateDatasetItemRequest) -> Result<DatasetItem, LangfuseError>;
            /// Blocking [`get`](crate::api::dataset_items::DatasetItemsClient::get).
            fn get(&self, id: &str) -> Result<DatasetItem, LangfuseError>;
            /// Blocking [`delete`](crate::api::dataset_items::DatasetItemsClient::delete).
            fn delete(&self, id: &str) -> Result<DeleteDatasetItemResponse, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::dataset_items::DatasetItemsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListDatasetItemsParams,
        ) -> BlockingPages<'_, Paginated<DatasetItem>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::datasets`].
pub mod datasets {
    #![allow(unused_imports)]
    pub use crate::api::datasets::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::datasets::{CreateDatasetRequest, Dataset};
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use chrono::{DateTime, Utc};
    use std::sync::Arc;

    /// Blocking version of [`DatasetsClient`](crate::api::datasets::DatasetsClient).
    #[derive(Clone)]
    pub struct DatasetsClient {
        pub(crate) inner: crate::api::datasets::DatasetsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for DatasetsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("DatasetsClient").finish_non_exhaustive()
        }
    }

    impl DatasetsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::datasets::DatasetsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<Dataset>, LangfuseError>;
            /// Blocking [`create`](crate::api::datasets::DatasetsClient::create).
            fn create(&self, req: &CreateDatasetRequest) -> Result<Dataset, LangfuseError>;
            /// Blocking [`get`](crate::api::datasets::DatasetsClient::get).
            fn get(&self, dataset_name: &str) -> Result<Dataset, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::datasets::DatasetsClient::list_pages): an iterator over the pages.
        pub fn list_pages(&self, params: PageParams) -> BlockingPages<'_, Paginated<Dataset>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::evaluation_rules`].
pub mod evaluation_rules {
    #![allow(unused_imports)]
    pub use crate::api::evaluation_rules::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::evaluation_rules::{
        CreateEvaluationRuleRequest, DeletedEvaluationRule, EvaluationRule,
        UpdateEvaluationRuleRequest,
    };
    use crate::pagination::{CursorPaginated, CursorParams, Pages, paginate_cursor};
    use std::sync::Arc;

    /// Blocking version of [`EvaluationRulesClient`](crate::api::evaluation_rules::EvaluationRulesClient).
    #[derive(Clone)]
    pub struct EvaluationRulesClient {
        pub(crate) inner: crate::api::evaluation_rules::EvaluationRulesClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for EvaluationRulesClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("EvaluationRulesClient")
                .finish_non_exhaustive()
        }
    }

    impl EvaluationRulesClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`create`](crate::api::evaluation_rules::EvaluationRulesClient::create).
            fn create(&self, req: &CreateEvaluationRuleRequest) -> Result<EvaluationRule, LangfuseError>;
            /// Blocking [`list`](crate::api::evaluation_rules::EvaluationRulesClient::list).
            fn list(&self, params: &CursorParams) -> Result<CursorPaginated<EvaluationRule>, LangfuseError>;
            /// Blocking [`get`](crate::api::evaluation_rules::EvaluationRulesClient::get).
            fn get(&self, rule_id: &str) -> Result<EvaluationRule, LangfuseError>;
            /// Blocking [`update`](crate::api::evaluation_rules::EvaluationRulesClient::update).
            fn update(&self, rule_id: &str, req: &UpdateEvaluationRuleRequest) -> Result<EvaluationRule, LangfuseError>;
            /// Blocking [`delete`](crate::api::evaluation_rules::EvaluationRulesClient::delete).
            fn delete(&self, rule_id: &str) -> Result<DeletedEvaluationRule, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::evaluation_rules::EvaluationRulesClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: CursorParams,
        ) -> BlockingPages<'_, CursorPaginated<EvaluationRule>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::evaluators`].
pub mod evaluators {
    #![allow(unused_imports)]
    pub use crate::api::evaluators::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::evaluators::{
        CreateEvaluatorRequest, DeletedEvaluator, Evaluator, EvaluatorVersion,
        UpdateEvaluatorRequest,
    };
    use crate::pagination::{CursorPaginated, CursorParams, Pages, paginate_cursor};
    use std::sync::Arc;

    /// Blocking version of [`EvaluatorsClient`](crate::api::evaluators::EvaluatorsClient).
    #[derive(Clone)]
    pub struct EvaluatorsClient {
        pub(crate) inner: crate::api::evaluators::EvaluatorsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for EvaluatorsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("EvaluatorsClient").finish_non_exhaustive()
        }
    }

    impl EvaluatorsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`create`](crate::api::evaluators::EvaluatorsClient::create).
            fn create(&self, req: &CreateEvaluatorRequest) -> Result<Evaluator, LangfuseError>;
            /// Blocking [`list`](crate::api::evaluators::EvaluatorsClient::list).
            fn list(&self, params: &CursorParams) -> Result<CursorPaginated<Evaluator>, LangfuseError>;
            /// Blocking [`get`](crate::api::evaluators::EvaluatorsClient::get).
            fn get(&self, evaluator_id: &str) -> Result<Evaluator, LangfuseError>;
            /// Blocking [`update`](crate::api::evaluators::EvaluatorsClient::update).
            fn update(&self, evaluator_id: &str, req: &UpdateEvaluatorRequest) -> Result<Evaluator, LangfuseError>;
            /// Blocking [`delete`](crate::api::evaluators::EvaluatorsClient::delete).
            fn delete(&self, evaluator_id: &str) -> Result<DeletedEvaluator, LangfuseError>;
            /// Blocking [`list_versions`](crate::api::evaluators::EvaluatorsClient::list_versions).
            fn list_versions(&self, evaluator_id: &str, params: &CursorParams) -> Result<CursorPaginated<EvaluatorVersion>, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::evaluators::EvaluatorsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: CursorParams,
        ) -> BlockingPages<'_, CursorPaginated<Evaluator>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }

        /// Blocking [`list_versions_pages`](crate::api::evaluators::EvaluatorsClient::list_versions_pages): an iterator over the pages.
        pub fn list_versions_pages<'a>(
            &'a self,
            evaluator_id: &'a str,
            params: CursorParams,
        ) -> BlockingPages<'a, CursorPaginated<EvaluatorVersion>> {
            BlockingPages::new(
                self.inner.list_versions_pages(evaluator_id, params),
                &self.runtime,
            )
        }
    }
}

/// Blocking clients of [`crate::api::experiments`].
pub mod experiments {
    #![allow(unused_imports)]
    pub use crate::api::experiments::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::HttpClient;
    use crate::models::experiments::{Experiment, ExperimentItem};
    use crate::pagination::{CursorPaginated, Pages, impl_cursor_query, paginate_cursor};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`ExperimentsClient`](crate::api::experiments::ExperimentsClient).
    #[derive(Clone)]
    pub struct ExperimentsClient {
        pub(crate) inner: crate::api::experiments::ExperimentsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ExperimentsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ExperimentsClient").finish_non_exhaustive()
        }
    }

    impl ExperimentsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::experiments::ExperimentsClient::list).
            fn list(&self, params: &ListExperimentsParams) -> Result<CursorPaginated<Experiment>, LangfuseError>;
            /// Blocking [`list_items`](crate::api::experiments::ExperimentsClient::list_items).
            fn list_items(&self, params: &ListExperimentItemsParams) -> Result<CursorPaginated<ExperimentItem>, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::experiments::ExperimentsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListExperimentsParams,
        ) -> BlockingPages<'_, CursorPaginated<Experiment>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }

        /// Blocking [`list_items_pages`](crate::api::experiments::ExperimentsClient::list_items_pages): an iterator over the pages.
        pub fn list_items_pages(
            &self,
            params: ListExperimentItemsParams,
        ) -> BlockingPages<'_, CursorPaginated<ExperimentItem>> {
            BlockingPages::new(self.inner.list_items_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::feedback`].
pub mod feedback {
    #![allow(unused_imports)]
    pub use crate::api::feedback::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::HttpClient;
    use std::sync::Arc;

    /// Blocking version of [`FeedbackClient`](crate::api::feedback::FeedbackClient).
    #[derive(Clone)]
    pub struct FeedbackClient {
        pub(crate) inner: crate::api::feedback::FeedbackClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for FeedbackClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("FeedbackClient").finish_non_exhaustive()
        }
    }

    impl FeedbackClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`submit`](crate::api::feedback::FeedbackClient::submit).
            fn submit(&self, req: &SubmitFeedbackRequest) -> Result<SubmitFeedbackResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::health`].
pub mod health {
    #![allow(unused_imports)]
    pub use crate::api::health::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::HttpClient;
    use crate::models::health::HealthResponse;
    use std::sync::Arc;

    /// Blocking version of [`HealthClient`](crate::api::health::HealthClient).
    #[derive(Clone)]
    pub struct HealthClient {
        pub(crate) inner: crate::api::health::HealthClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for HealthClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("HealthClient").finish_non_exhaustive()
        }
    }

    impl HealthClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`check`](crate::api::health::HealthClient::check).
            fn check(&self) -> Result<HealthResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::ingestion`].
pub mod ingestion {
    #![allow(unused_imports)]
    pub use crate::api::ingestion::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{CallOptions, HttpClient};
    use crate::models::ingestion::IngestTracesRequest;
    use reqwest::header::{HeaderMap, HeaderValue};
    use std::sync::Arc;

    /// Blocking version of [`IngestionClient`](crate::api::ingestion::IngestionClient).
    #[derive(Clone)]
    pub struct IngestionClient {
        pub(crate) inner: crate::api::ingestion::IngestionClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for IngestionClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("IngestionClient").finish_non_exhaustive()
        }
    }

    impl IngestionClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`ingest_traces`](crate::api::ingestion::IngestionClient::ingest_traces).
            fn ingest_traces(&self, req: &IngestTracesRequest) -> Result<(), LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::llm_connections`].
pub mod llm_connections {
    #![allow(unused_imports)]
    pub use crate::api::llm_connections::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::llm_connections::{
        DeleteLlmConnectionResponse, LlmConnection, UpsertLlmConnectionRequest,
    };
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use std::sync::Arc;

    /// Blocking version of [`LlmConnectionsClient`](crate::api::llm_connections::LlmConnectionsClient).
    #[derive(Clone)]
    pub struct LlmConnectionsClient {
        pub(crate) inner: crate::api::llm_connections::LlmConnectionsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for LlmConnectionsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("LlmConnectionsClient")
                .finish_non_exhaustive()
        }
    }

    impl LlmConnectionsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::llm_connections::LlmConnectionsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<LlmConnection>, LangfuseError>;
            /// Blocking [`upsert`](crate::api::llm_connections::LlmConnectionsClient::upsert).
            fn upsert(&self, req: &UpsertLlmConnectionRequest) -> Result<LlmConnection, LangfuseError>;
            /// Blocking [`delete`](crate::api::llm_connections::LlmConnectionsClient::delete).
            fn delete(&self, id: &str) -> Result<DeleteLlmConnectionResponse, LangfuseError>;
            /// Blocking [`list_organization`](crate::api::llm_connections::LlmConnectionsClient::list_organization).
            fn list_organization(&self, params: &PageParams) -> Result<Paginated<LlmConnection>, LangfuseError>;
            /// Blocking [`upsert_organization`](crate::api::llm_connections::LlmConnectionsClient::upsert_organization).
            fn upsert_organization(&self, req: &UpsertLlmConnectionRequest) -> Result<LlmConnection, LangfuseError>;
            /// Blocking [`delete_organization`](crate::api::llm_connections::LlmConnectionsClient::delete_organization).
            fn delete_organization(&self, id: &str) -> Result<DeleteLlmConnectionResponse, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::llm_connections::LlmConnectionsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: PageParams,
        ) -> BlockingPages<'_, Paginated<LlmConnection>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }

        /// Blocking [`list_organization_pages`](crate::api::llm_connections::LlmConnectionsClient::list_organization_pages): an iterator over the pages.
        pub fn list_organization_pages(
            &self,
            params: PageParams,
        ) -> BlockingPages<'_, Paginated<LlmConnection>> {
            BlockingPages::new(self.inner.list_organization_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::media`].
pub mod media {
    #![allow(unused_imports)]
    pub use crate::api::media::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::media::{DOWNLOAD_TIMEOUT, MAX_DEPTH, MediaReference, find_references};
    use crate::models::media::{
        GetMediaResponse, GetMediaUploadUrlRequest, GetMediaUploadUrlResponse, PatchMediaBody,
    };
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;
    use serde_json::Value;
    use std::collections::{BTreeSet, HashMap};
    use std::sync::Arc;

    /// Blocking version of [`MediaClient`](crate::api::media::MediaClient).
    #[derive(Clone)]
    pub struct MediaClient {
        pub(crate) inner: crate::api::media::MediaClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for MediaClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MediaClient").finish_non_exhaustive()
        }
    }

    impl MediaClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`get`](crate::api::media::MediaClient::get).
            fn get(&self, media_id: &str) -> Result<GetMediaResponse, LangfuseError>;
            /// Blocking [`patch`](crate::api::media::MediaClient::patch).
            fn patch(&self, media_id: &str, body: &PatchMediaBody) -> Result<(), LangfuseError>;
            /// Blocking [`get_upload_url`](crate::api::media::MediaClient::get_upload_url).
            fn get_upload_url(&self, req: &GetMediaUploadUrlRequest) -> Result<GetMediaUploadUrlResponse, LangfuseError>;
            /// Blocking [`download`](crate::api::media::MediaClient::download).
            fn download(&self, media_id: &str) -> Result<DownloadedMedia, LangfuseError>;
            /// Blocking [`resolve_references`](crate::api::media::MediaClient::resolve_references).
            fn resolve_references(&self, value: &Value) -> Value;
        }
    }
}

/// Blocking clients of [`crate::api::metrics`].
pub mod metrics {
    #![allow(unused_imports)]
    pub use crate::api::metrics::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::HttpClient;
    use crate::models::metrics::{MetricsQuery, MetricsResponse};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`MetricsClient`](crate::api::metrics::MetricsClient).
    #[derive(Clone)]
    pub struct MetricsClient {
        pub(crate) inner: crate::api::metrics::MetricsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for MetricsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("MetricsClient").finish_non_exhaustive()
        }
    }

    impl MetricsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`query`](crate::api::metrics::MetricsClient::query).
            fn query(&self, query: &MetricsQuery) -> Result<MetricsResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::models`].
pub mod models {
    #![allow(unused_imports)]
    pub use crate::api::models::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::models_api::{CreateModelRequest, Model};
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use std::sync::Arc;

    /// Blocking version of [`ModelsClient`](crate::api::models::ModelsClient).
    #[derive(Clone)]
    pub struct ModelsClient {
        pub(crate) inner: crate::api::models::ModelsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ModelsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ModelsClient").finish_non_exhaustive()
        }
    }

    impl ModelsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::models::ModelsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<Model>, LangfuseError>;
            /// Blocking [`create`](crate::api::models::ModelsClient::create).
            fn create(&self, req: &CreateModelRequest) -> Result<Model, LangfuseError>;
            /// Blocking [`get`](crate::api::models::ModelsClient::get).
            fn get(&self, id: &str) -> Result<Model, LangfuseError>;
            /// Blocking [`update`](crate::api::models::ModelsClient::update).
            fn update(&self, id: &str, req: &CreateModelRequest) -> Result<Model, LangfuseError>;
            /// Blocking [`delete`](crate::api::models::ModelsClient::delete).
            fn delete(&self, id: &str) -> Result<(), LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::models::ModelsClient::list_pages): an iterator over the pages.
        pub fn list_pages(&self, params: PageParams) -> BlockingPages<'_, Paginated<Model>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::observations`].
pub mod observations {
    #![allow(unused_imports)]
    pub use crate::api::observations::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{CallOptions, HttpClient};
    use crate::models::observations::ObservationV2;
    use crate::pagination::{CursorPaginated, Pages, impl_cursor_query, paginate_cursor};
    use serde::{Serialize, Serializer};
    use std::sync::Arc;
    use std::time::Duration;

    /// Blocking version of [`ObservationsClient`](crate::api::observations::ObservationsClient).
    #[derive(Clone)]
    pub struct ObservationsClient {
        pub(crate) inner: crate::api::observations::ObservationsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ObservationsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ObservationsClient").finish_non_exhaustive()
        }
    }

    impl ObservationsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::observations::ObservationsClient::list).
            fn list(&self, params: &ListObservationsParams) -> Result<CursorPaginated<ObservationV2>, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::observations::ObservationsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListObservationsParams,
        ) -> BlockingPages<'_, CursorPaginated<ObservationV2>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::organizations`].
pub mod organizations {
    #![allow(unused_imports)]
    pub use crate::api::organizations::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::organizations::{
        DeleteMembershipRequest, MembershipDeletionResponse, MembershipRequest, MembershipResponse,
        MembershipsResponse, OrganizationApiKeysResponse, OrganizationProjectsResponse,
    };
    use std::sync::Arc;

    /// Blocking version of [`OrganizationsClient`](crate::api::organizations::OrganizationsClient).
    #[derive(Clone)]
    pub struct OrganizationsClient {
        pub(crate) inner: crate::api::organizations::OrganizationsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for OrganizationsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("OrganizationsClient")
                .finish_non_exhaustive()
        }
    }

    impl OrganizationsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`get_memberships`](crate::api::organizations::OrganizationsClient::get_memberships).
            fn get_memberships(&self) -> Result<MembershipsResponse, LangfuseError>;
            /// Blocking [`update_membership`](crate::api::organizations::OrganizationsClient::update_membership).
            fn update_membership(&self, req: &MembershipRequest) -> Result<MembershipResponse, LangfuseError>;
            /// Blocking [`delete_membership`](crate::api::organizations::OrganizationsClient::delete_membership).
            fn delete_membership(&self, user_id: &str) -> Result<MembershipDeletionResponse, LangfuseError>;
            /// Blocking [`get_project_memberships`](crate::api::organizations::OrganizationsClient::get_project_memberships).
            fn get_project_memberships(&self, project_id: &str) -> Result<MembershipsResponse, LangfuseError>;
            /// Blocking [`update_project_membership`](crate::api::organizations::OrganizationsClient::update_project_membership).
            fn update_project_membership(&self, project_id: &str, req: &MembershipRequest) -> Result<MembershipResponse, LangfuseError>;
            /// Blocking [`delete_project_membership`](crate::api::organizations::OrganizationsClient::delete_project_membership).
            fn delete_project_membership(&self, project_id: &str, user_id: &str) -> Result<MembershipDeletionResponse, LangfuseError>;
            /// Blocking [`get_projects`](crate::api::organizations::OrganizationsClient::get_projects).
            fn get_projects(&self) -> Result<OrganizationProjectsResponse, LangfuseError>;
            /// Blocking [`get_api_keys`](crate::api::organizations::OrganizationsClient::get_api_keys).
            fn get_api_keys(&self) -> Result<OrganizationApiKeysResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::projects`].
pub mod projects {
    #![allow(unused_imports)]
    pub use crate::api::projects::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::projects::{
        ApiKeyDeletionResponse, ApiKeyList, ApiKeyResponse, Project, ProjectDeletionResponse,
    };
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;

    /// Blocking version of [`ProjectsClient`](crate::api::projects::ProjectsClient).
    #[derive(Clone)]
    pub struct ProjectsClient {
        pub(crate) inner: crate::api::projects::ProjectsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ProjectsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ProjectsClient").finish_non_exhaustive()
        }
    }

    impl ProjectsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::projects::ProjectsClient::list).
            fn list(&self) -> Result<ProjectsListResponse, LangfuseError>;
            /// Blocking [`create`](crate::api::projects::ProjectsClient::create).
            fn create(&self, req: &CreateProjectRequest) -> Result<Project, LangfuseError>;
            /// Blocking [`update`](crate::api::projects::ProjectsClient::update).
            fn update(&self, project_id: &str, req: &UpdateProjectRequest) -> Result<Project, LangfuseError>;
            /// Blocking [`delete`](crate::api::projects::ProjectsClient::delete).
            fn delete(&self, project_id: &str) -> Result<ProjectDeletionResponse, LangfuseError>;
            /// Blocking [`get_api_keys`](crate::api::projects::ProjectsClient::get_api_keys).
            fn get_api_keys(&self, project_id: &str) -> Result<ApiKeyList, LangfuseError>;
            /// Blocking [`create_api_key`](crate::api::projects::ProjectsClient::create_api_key).
            fn create_api_key(&self, project_id: &str, req: &CreateApiKeyRequest) -> Result<ApiKeyResponse, LangfuseError>;
            /// Blocking [`delete_api_key`](crate::api::projects::ProjectsClient::delete_api_key).
            fn delete_api_key(&self, project_id: &str, api_key_id: &str) -> Result<ApiKeyDeletionResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::prompt_version`].
pub mod prompt_version {
    #![allow(unused_imports)]
    pub use crate::api::prompt_version::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::prompts::Prompt;
    use crate::prompt_cache::PromptCache;
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`PromptVersionClient`](crate::api::prompt_version::PromptVersionClient).
    #[derive(Clone)]
    pub struct PromptVersionClient {
        pub(crate) inner: crate::api::prompt_version::PromptVersionClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for PromptVersionClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PromptVersionClient")
                .finish_non_exhaustive()
        }
    }

    impl PromptVersionClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`update`](crate::api::prompt_version::PromptVersionClient::update).
            fn update(&self, name: &str, version: i64, req: &UpdatePromptVersionRequest) -> Result<Prompt, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::prompts`].
pub mod prompts {
    #![allow(unused_imports)]
    pub use crate::api::prompts::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{CallOptions, HttpClient, encode_path_segment};
    use crate::models::enums::PromptType;
    use crate::models::prompts::{BasePrompt, ChatPromptMessage, CreatePromptRequest, Prompt};
    use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};
    use crate::prompt_cache::{CacheKey, Lookup, PromptCache};
    use chrono::{DateTime, Utc};
    use serde::{Deserialize, Serialize};
    use std::sync::Arc;
    use std::time::Duration;

    /// Blocking version of [`PromptsClient`](crate::api::prompts::PromptsClient).
    #[derive(Clone)]
    pub struct PromptsClient {
        pub(crate) inner: crate::api::prompts::PromptsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for PromptsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PromptsClient").finish_non_exhaustive()
        }
    }

    impl PromptsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::prompts::PromptsClient::list).
            fn list(&self, params: &ListPromptsParams) -> Result<Paginated<PromptMeta>, LangfuseError>;
            /// Blocking [`create`](crate::api::prompts::PromptsClient::create).
            fn create(&self, req: &CreatePromptRequest) -> Result<Prompt, LangfuseError>;
            /// Blocking [`get`](crate::api::prompts::PromptsClient::get).
            fn get(&self, name: &str) -> Result<Prompt, LangfuseError>;
            /// Blocking [`get_with_params`](crate::api::prompts::PromptsClient::get_with_params).
            fn get_with_params(&self, name: &str, params: &GetPromptParams) -> Result<Prompt, LangfuseError>;
            /// Blocking [`delete`](crate::api::prompts::PromptsClient::delete).
            fn delete(&self, name: &str) -> Result<(), LangfuseError>;
            /// Blocking [`delete_with_params`](crate::api::prompts::PromptsClient::delete_with_params).
            fn delete_with_params(&self, name: &str, params: &DeletePromptParams) -> Result<(), LangfuseError>;
            /// Blocking [`get_cached`](crate::api::prompts::PromptsClient::get_cached).
            fn get_cached(&self, name: &str, options: &GetPromptOptions) -> Result<Arc<Prompt>, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::prompts::PromptsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListPromptsParams,
        ) -> BlockingPages<'_, Paginated<PromptMeta>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }

        /// See [`clear_cache`](crate::api::prompts::PromptsClient::clear_cache).
        pub fn clear_cache(&self) {
            self.inner.clear_cache()
        }
    }
}

/// Blocking clients of [`crate::api::scim`].
pub mod scim {
    #![allow(unused_imports)]
    pub use crate::api::scim::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::scim::{
        CreateScimUserRequest, ResourceTypesResponse, SchemasResponse, ScimUser,
        ScimUsersListResponse, ServiceProviderConfig,
    };
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`ScimClient`](crate::api::scim::ScimClient).
    #[derive(Clone)]
    pub struct ScimClient {
        pub(crate) inner: crate::api::scim::ScimClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ScimClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ScimClient").finish_non_exhaustive()
        }
    }

    impl ScimClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`get_service_provider_config`](crate::api::scim::ScimClient::get_service_provider_config).
            fn get_service_provider_config(&self) -> Result<ServiceProviderConfig, LangfuseError>;
            /// Blocking [`get_resource_types`](crate::api::scim::ScimClient::get_resource_types).
            fn get_resource_types(&self) -> Result<ResourceTypesResponse, LangfuseError>;
            /// Blocking [`get_schemas`](crate::api::scim::ScimClient::get_schemas).
            fn get_schemas(&self) -> Result<SchemasResponse, LangfuseError>;
            /// Blocking [`list_users`](crate::api::scim::ScimClient::list_users).
            fn list_users(&self, params: &ListScimUsersParams) -> Result<ScimUsersListResponse, LangfuseError>;
            /// Blocking [`create_user`](crate::api::scim::ScimClient::create_user).
            fn create_user(&self, req: &CreateScimUserRequest) -> Result<ScimUser, LangfuseError>;
            /// Blocking [`get_user`](crate::api::scim::ScimClient::get_user).
            fn get_user(&self, user_id: &str) -> Result<ScimUser, LangfuseError>;
            /// Blocking [`delete_user`](crate::api::scim::ScimClient::delete_user).
            fn delete_user(&self, user_id: &str) -> Result<(), LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::score_configs`].
pub mod score_configs {
    #![allow(unused_imports)]
    pub use crate::api::score_configs::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::score_configs::{
        CreateScoreConfigRequest, ScoreConfig, UpdateScoreConfigRequest,
    };
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use std::sync::Arc;

    /// Blocking version of [`ScoreConfigsClient`](crate::api::score_configs::ScoreConfigsClient).
    #[derive(Clone)]
    pub struct ScoreConfigsClient {
        pub(crate) inner: crate::api::score_configs::ScoreConfigsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ScoreConfigsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ScoreConfigsClient").finish_non_exhaustive()
        }
    }

    impl ScoreConfigsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::score_configs::ScoreConfigsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<ScoreConfig>, LangfuseError>;
            /// Blocking [`create`](crate::api::score_configs::ScoreConfigsClient::create).
            fn create(&self, req: &CreateScoreConfigRequest) -> Result<ScoreConfig, LangfuseError>;
            /// Blocking [`get`](crate::api::score_configs::ScoreConfigsClient::get).
            fn get(&self, config_id: &str) -> Result<ScoreConfig, LangfuseError>;
            /// Blocking [`update`](crate::api::score_configs::ScoreConfigsClient::update).
            fn update(&self, config_id: &str, req: &UpdateScoreConfigRequest) -> Result<ScoreConfig, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::score_configs::ScoreConfigsClient::list_pages): an iterator over the pages.
        pub fn list_pages(&self, params: PageParams) -> BlockingPages<'_, Paginated<ScoreConfig>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::scores`].
pub mod scores {
    #![allow(unused_imports)]
    pub use crate::api::scores::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::scores::{CreateScoreRequest, CreateScoresResponse, ScoreV3};
    use crate::pagination::{CursorPaginated, Pages, impl_cursor_query, paginate_cursor};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`ScoresClient`](crate::api::scores::ScoresClient).
    #[derive(Clone)]
    pub struct ScoresClient {
        pub(crate) inner: crate::api::scores::ScoresClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for ScoresClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("ScoresClient").finish_non_exhaustive()
        }
    }

    impl ScoresClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::scores::ScoresClient::list).
            fn list(&self, params: &ListScoresParams) -> Result<CursorPaginated<ScoreV3>, LangfuseError>;
            /// Blocking [`create`](crate::api::scores::ScoresClient::create).
            fn create(&self, scores: &[CreateScoreRequest]) -> Result<CreateScoresResponse, LangfuseError>;
            /// Blocking [`delete`](crate::api::scores::ScoresClient::delete).
            fn delete(&self, score_id: &str) -> Result<(), LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::scores::ScoresClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: ListScoresParams,
        ) -> BlockingPages<'_, CursorPaginated<ScoreV3>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::skills`].
pub mod skills {
    #![allow(unused_imports)]
    pub use crate::api::skills::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::skills::{
        CreateSkillVersionRequest, DeleteSkillVersionResponse, SkillFileContentsResponse,
        SkillListResponse, SkillVersion,
    };
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`SkillsClient`](crate::api::skills::SkillsClient).
    #[derive(Clone)]
    pub struct SkillsClient {
        pub(crate) inner: crate::api::skills::SkillsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for SkillsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("SkillsClient").finish_non_exhaustive()
        }
    }

    impl SkillsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::skills::SkillsClient::list).
            fn list(&self, params: &ListSkillsParams) -> Result<SkillListResponse, LangfuseError>;
            /// Blocking [`create_version`](crate::api::skills::SkillsClient::create_version).
            fn create_version(&self, req: &CreateSkillVersionRequest) -> Result<SkillVersion, LangfuseError>;
            /// Blocking [`get`](crate::api::skills::SkillsClient::get).
            fn get(&self, name: &str, params: &GetSkillParams) -> Result<SkillVersion, LangfuseError>;
            /// Blocking [`update`](crate::api::skills::SkillsClient::update).
            fn update(&self, name: &str, req: &UpdateSkillRequest) -> Result<SkillVersion, LangfuseError>;
            /// Blocking [`get_file_contents`](crate::api::skills::SkillsClient::get_file_contents).
            fn get_file_contents(&self, sha256_hashes: &[&str]) -> Result<SkillFileContentsResponse, LangfuseError>;
            /// Blocking [`set_version_labels`](crate::api::skills::SkillsClient::set_version_labels).
            fn set_version_labels(&self, name: &str, version: i64, req: &SetSkillVersionLabelsRequest) -> Result<SkillVersion, LangfuseError>;
            /// Blocking [`delete_version`](crate::api::skills::SkillsClient::delete_version).
            fn delete_version(&self, name: &str, version: i64) -> Result<DeleteSkillVersionResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::traces`].
pub mod traces {
    #![allow(unused_imports)]
    pub use crate::api::traces::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::traces::{DeleteTraceResponse, DeleteTracesRequest};
    use std::sync::Arc;

    /// Blocking version of [`TracesClient`](crate::api::traces::TracesClient).
    #[derive(Clone)]
    pub struct TracesClient {
        pub(crate) inner: crate::api::traces::TracesClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for TracesClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("TracesClient").finish_non_exhaustive()
        }
    }

    impl TracesClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`delete`](crate::api::traces::TracesClient::delete).
            fn delete(&self, trace_id: &str) -> Result<DeleteTraceResponse, LangfuseError>;
            /// Blocking [`delete_batch`](crate::api::traces::TracesClient::delete_batch).
            fn delete_batch(&self, req: &DeleteTracesRequest) -> Result<DeleteTraceResponse, LangfuseError>;
        }
    }
}

/// Blocking clients of [`crate::api::unstable_dashboard_widgets`].
#[cfg(feature = "unstable")]
pub mod unstable_dashboard_widgets {
    #![allow(unused_imports)]
    pub use crate::api::unstable_dashboard_widgets::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::unstable_dashboard_widgets::{
        CreateDashboardWidgetRequest, DashboardWidget, DeleteDashboardWidgetResponse,
        UpdateDashboardWidgetRequest,
    };
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use std::sync::Arc;

    /// Blocking version of [`UnstableDashboardWidgetsClient`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient).
    #[derive(Clone)]
    pub struct UnstableDashboardWidgetsClient {
        pub(crate) inner: crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for UnstableDashboardWidgetsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("UnstableDashboardWidgetsClient")
                .finish_non_exhaustive()
        }
    }

    impl UnstableDashboardWidgetsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<DashboardWidget>, LangfuseError>;
            /// Blocking [`create`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::create).
            fn create(&self, req: &CreateDashboardWidgetRequest) -> Result<DashboardWidget, LangfuseError>;
            /// Blocking [`get`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::get).
            fn get(&self, widget_id: &str) -> Result<DashboardWidget, LangfuseError>;
            /// Blocking [`update`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::update).
            fn update(&self, widget_id: &str, req: &UpdateDashboardWidgetRequest) -> Result<DashboardWidget, LangfuseError>;
            /// Blocking [`delete`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::delete).
            fn delete(&self, widget_id: &str) -> Result<DeleteDashboardWidgetResponse, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::unstable_dashboard_widgets::UnstableDashboardWidgetsClient::list_pages): an iterator over the pages.
        pub fn list_pages(
            &self,
            params: PageParams,
        ) -> BlockingPages<'_, Paginated<DashboardWidget>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::unstable_dashboards`].
#[cfg(feature = "unstable")]
pub mod unstable_dashboards {
    #![allow(unused_imports)]
    pub use crate::api::unstable_dashboards::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::unstable_dashboards::{
        CreateDashboardPlacementRequest, CreateDashboardRequest, Dashboard, DashboardPlacement,
        DeleteDashboardPlacementResponse, DeleteDashboardResponse, UpdateDashboardPlacementRequest,
        UpdateDashboardRequest,
    };
    use crate::pagination::{PageParams, Pages, Paginated, paginate_pages};
    use std::sync::Arc;

    /// Blocking version of [`UnstableDashboardsClient`](crate::api::unstable_dashboards::UnstableDashboardsClient).
    #[derive(Clone)]
    pub struct UnstableDashboardsClient {
        pub(crate) inner: crate::api::unstable_dashboards::UnstableDashboardsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for UnstableDashboardsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("UnstableDashboardsClient")
                .finish_non_exhaustive()
        }
    }

    impl UnstableDashboardsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list`](crate::api::unstable_dashboards::UnstableDashboardsClient::list).
            fn list(&self, params: &PageParams) -> Result<Paginated<Dashboard>, LangfuseError>;
            /// Blocking [`create`](crate::api::unstable_dashboards::UnstableDashboardsClient::create).
            fn create(&self, req: &CreateDashboardRequest) -> Result<Dashboard, LangfuseError>;
            /// Blocking [`get`](crate::api::unstable_dashboards::UnstableDashboardsClient::get).
            fn get(&self, dashboard_id: &str) -> Result<Dashboard, LangfuseError>;
            /// Blocking [`update`](crate::api::unstable_dashboards::UnstableDashboardsClient::update).
            fn update(&self, dashboard_id: &str, req: &UpdateDashboardRequest) -> Result<Dashboard, LangfuseError>;
            /// Blocking [`delete`](crate::api::unstable_dashboards::UnstableDashboardsClient::delete).
            fn delete(&self, dashboard_id: &str) -> Result<DeleteDashboardResponse, LangfuseError>;
            /// Blocking [`add_placement`](crate::api::unstable_dashboards::UnstableDashboardsClient::add_placement).
            fn add_placement(&self, dashboard_id: &str, req: &CreateDashboardPlacementRequest) -> Result<DashboardPlacement, LangfuseError>;
            /// Blocking [`update_placement`](crate::api::unstable_dashboards::UnstableDashboardsClient::update_placement).
            fn update_placement(&self, dashboard_id: &str, placement_id: &str, req: &UpdateDashboardPlacementRequest) -> Result<DashboardPlacement, LangfuseError>;
            /// Blocking [`delete_placement`](crate::api::unstable_dashboards::UnstableDashboardsClient::delete_placement).
            fn delete_placement(&self, dashboard_id: &str, placement_id: &str) -> Result<DeleteDashboardPlacementResponse, LangfuseError>;
        }

        /// Blocking [`list_pages`](crate::api::unstable_dashboards::UnstableDashboardsClient::list_pages): an iterator over the pages.
        pub fn list_pages(&self, params: PageParams) -> BlockingPages<'_, Paginated<Dashboard>> {
            BlockingPages::new(self.inner.list_pages(params), &self.runtime)
        }
    }
}

/// Blocking clients of [`crate::api::unstable_topics`].
#[cfg(feature = "unstable")]
pub mod unstable_topics {
    #![allow(unused_imports)]
    pub use crate::api::unstable_topics::*;
    use crate::blocking::{BlockingPages, Runtime};
    use crate::error::LangfuseError;
    use crate::http::{HttpClient, encode_path_segment};
    use crate::models::unstable_topics::{
        TopicAssignmentList, TopicFacet, TopicList, TopicResultsOrderBy, TopicRule,
        TopicSummaryList, TopicWithStatus, TopicsOrderBy,
    };
    use crate::pagination::{Pages, Paginated, impl_page_query, paginate_pages};
    use serde::Serialize;
    use std::sync::Arc;

    /// Blocking version of [`UnstableTopicsClient`](crate::api::unstable_topics::UnstableTopicsClient).
    #[derive(Clone)]
    pub struct UnstableTopicsClient {
        pub(crate) inner: crate::api::unstable_topics::UnstableTopicsClient,
        pub(crate) runtime: std::sync::Arc<Runtime>,
    }

    impl std::fmt::Debug for UnstableTopicsClient {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("UnstableTopicsClient")
                .finish_non_exhaustive()
        }
    }

    impl UnstableTopicsClient {
        crate::blocking::blocking_methods! {
            /// Blocking [`list_facets`](crate::api::unstable_topics::UnstableTopicsClient::list_facets).
            fn list_facets(&self, params: &ListTopicFacetsParams) -> Result<Paginated<TopicFacet>, LangfuseError>;
            /// Blocking [`get_facet`](crate::api::unstable_topics::UnstableTopicsClient::get_facet).
            fn get_facet(&self, facet_id: &str) -> Result<TopicFacet, LangfuseError>;
            /// Blocking [`list`](crate::api::unstable_topics::UnstableTopicsClient::list).
            fn list(&self, params: &ListTopicsParams) -> Result<TopicList, LangfuseError>;
            /// Blocking [`get`](crate::api::unstable_topics::UnstableTopicsClient::get).
            fn get(&self, topic_id: &str, params: &GetTopicParams) -> Result<TopicWithStatus, LangfuseError>;
            /// Blocking [`list_rules`](crate::api::unstable_topics::UnstableTopicsClient::list_rules).
            fn list_rules(&self, params: &ListTopicRulesParams) -> Result<Paginated<TopicRule>, LangfuseError>;
            /// Blocking [`get_rule`](crate::api::unstable_topics::UnstableTopicsClient::get_rule).
            fn get_rule(&self, rule_id: &str) -> Result<TopicRule, LangfuseError>;
            /// Blocking [`list_summaries`](crate::api::unstable_topics::UnstableTopicsClient::list_summaries).
            fn list_summaries(&self, params: &ListTopicSummariesParams) -> Result<TopicSummaryList, LangfuseError>;
            /// Blocking [`list_assignments`](crate::api::unstable_topics::UnstableTopicsClient::list_assignments).
            fn list_assignments(&self, params: &ListTopicAssignmentsParams) -> Result<TopicAssignmentList, LangfuseError>;
        }

        /// Blocking [`list_facets_pages`](crate::api::unstable_topics::UnstableTopicsClient::list_facets_pages): an iterator over the pages.
        pub fn list_facets_pages(
            &self,
            params: ListTopicFacetsParams,
        ) -> BlockingPages<'_, Paginated<TopicFacet>> {
            BlockingPages::new(self.inner.list_facets_pages(params), &self.runtime)
        }

        /// Blocking [`list_rules_pages`](crate::api::unstable_topics::UnstableTopicsClient::list_rules_pages): an iterator over the pages.
        pub fn list_rules_pages(
            &self,
            params: ListTopicRulesParams,
        ) -> BlockingPages<'_, Paginated<TopicRule>> {
            BlockingPages::new(self.inner.list_rules_pages(params), &self.runtime)
        }
    }
}

/// Blocking entry point of the SDK: the same sub-clients as
/// [`crate::LangfuseClient`], whose methods block until the request finishes.
///
/// See the [module docs](crate::blocking) for the runtime it runs on.
#[derive(Clone)]
#[non_exhaustive]
pub struct LangfuseClient {
    /// Health-check endpoint.
    pub health: health::HealthClient,
    /// Create, list, update and delete scores.
    pub scores: scores::ScoresClient,
    /// Manage score configuration presets.
    pub score_configs: score_configs::ScoreConfigsClient,
    /// Query observations (spans, generations, events).
    pub observations: observations::ObservationsClient,
    /// Create and manage datasets.
    pub datasets: datasets::DatasetsClient,
    /// Create and manage dataset items.
    pub dataset_items: dataset_items::DatasetItemsClient,
    /// Retrieve and manage prompts.
    pub prompts: prompts::PromptsClient,
    /// Manage individual prompt versions.
    pub prompt_versions: prompt_version::PromptVersionClient,
    /// CRUD operations on LLM model definitions.
    pub models: models::ModelsClient,
    /// Manage skills.
    pub skills: skills::SkillsClient,
    /// Project-level operations.
    pub projects: projects::ProjectsClient,
    /// Create and query traces.
    pub traces: traces::TracesClient,
    /// Batch ingestion of traces, spans, generations, scores and events.
    pub ingestion: ingestion::IngestionClient,
    /// Upload and manage media assets.
    pub media: media::MediaClient,
    /// Manage comments on traces and observations.
    pub comments: comments::CommentsClient,
    /// Submit end-user feedback.
    pub feedback: feedback::FeedbackClient,
    /// Manage annotation queues and their items.
    pub annotation_queues: annotation_queues::AnnotationQueuesClient,
    /// Blob storage integration configuration.
    pub blob_storage_integrations: blob_storage_integrations::BlobStorageIntegrationsClient,
    /// Manage automated evaluation rules.
    pub evaluation_rules: evaluation_rules::EvaluationRulesClient,
    /// Manage evaluator definitions.
    pub evaluators: evaluators::EvaluatorsClient,
    /// Manage experiments and their runs.
    pub experiments: experiments::ExperimentsClient,
    /// Manage LLM provider connections.
    pub llm_connections: llm_connections::LlmConnectionsClient,
    /// Query aggregated metrics.
    pub metrics: metrics::MetricsClient,
    /// Organization-level operations.
    pub organizations: organizations::OrganizationsClient,
    /// SCIM 2.0 user provisioning.
    pub scim: scim::ScimClient,
    /// *(unstable)* Dashboard widget queries.
    #[cfg(feature = "unstable")]
    pub unstable_dashboard_widgets: unstable_dashboard_widgets::UnstableDashboardWidgetsClient,
    /// *(unstable)* Dashboard CRUD.
    #[cfg(feature = "unstable")]
    pub unstable_dashboards: unstable_dashboards::UnstableDashboardsClient,
    /// *(unstable)* Topic management.
    #[cfg(feature = "unstable")]
    pub unstable_topics: unstable_topics::UnstableTopicsClient,
    pub(crate) inner: crate::LangfuseClient,
    pub(crate) runtime: std::sync::Arc<Runtime>,
}

impl LangfuseClient {
    /// Wrap `client`, running its requests on `runtime`.
    pub(crate) fn assemble(
        client: crate::LangfuseClient,
        runtime: std::sync::Arc<Runtime>,
    ) -> Self {
        Self {
            health: health::HealthClient {
                inner: client.health.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            scores: scores::ScoresClient {
                inner: client.scores.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            score_configs: score_configs::ScoreConfigsClient {
                inner: client.score_configs.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            observations: observations::ObservationsClient {
                inner: client.observations.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            datasets: datasets::DatasetsClient {
                inner: client.datasets.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            dataset_items: dataset_items::DatasetItemsClient {
                inner: client.dataset_items.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            prompts: prompts::PromptsClient {
                inner: client.prompts.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            prompt_versions: prompt_version::PromptVersionClient {
                inner: client.prompt_versions.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            models: models::ModelsClient {
                inner: client.models.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            skills: skills::SkillsClient {
                inner: client.skills.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            projects: projects::ProjectsClient {
                inner: client.projects.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            traces: traces::TracesClient {
                inner: client.traces.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            ingestion: ingestion::IngestionClient {
                inner: client.ingestion.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            media: media::MediaClient {
                inner: client.media.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            comments: comments::CommentsClient {
                inner: client.comments.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            feedback: feedback::FeedbackClient {
                inner: client.feedback.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            annotation_queues: annotation_queues::AnnotationQueuesClient {
                inner: client.annotation_queues.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            blob_storage_integrations: blob_storage_integrations::BlobStorageIntegrationsClient {
                inner: client.blob_storage_integrations.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            evaluation_rules: evaluation_rules::EvaluationRulesClient {
                inner: client.evaluation_rules.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            evaluators: evaluators::EvaluatorsClient {
                inner: client.evaluators.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            experiments: experiments::ExperimentsClient {
                inner: client.experiments.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            llm_connections: llm_connections::LlmConnectionsClient {
                inner: client.llm_connections.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            metrics: metrics::MetricsClient {
                inner: client.metrics.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            organizations: organizations::OrganizationsClient {
                inner: client.organizations.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            scim: scim::ScimClient {
                inner: client.scim.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            #[cfg(feature = "unstable")]
            unstable_dashboard_widgets:
                unstable_dashboard_widgets::UnstableDashboardWidgetsClient {
                    inner: client.unstable_dashboard_widgets.clone(),
                    runtime: std::sync::Arc::clone(&runtime),
                },
            #[cfg(feature = "unstable")]
            unstable_dashboards: unstable_dashboards::UnstableDashboardsClient {
                inner: client.unstable_dashboards.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            #[cfg(feature = "unstable")]
            unstable_topics: unstable_topics::UnstableTopicsClient {
                inner: client.unstable_topics.clone(),
                runtime: std::sync::Arc::clone(&runtime),
            },
            inner: client,
            runtime,
        }
    }
}

use crate::blocking::Runtime;
