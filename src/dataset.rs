//! Dataset helpers of [`LangfuseClient`]: fetch a dataset with every item, and
//! create items whose media is uploaded first.

use futures_util::TryStreamExt;

use crate::LangfuseClient;
use crate::api::dataset_items::ListDatasetItemsParams;
use crate::api::datasets::GetDatasetOptions;
use crate::error::LangfuseError;
use crate::experiment::python_isoformat;
use crate::media::{LangfuseMedia, MediaField, MediaTarget, extract_media};
use crate::models::dataset_items::{CreateDatasetItemRequest, DatasetItem};
use crate::models::datasets::DatasetWithItems;

impl LangfuseClient {
    /// Fetch dataset `name` with all its items: `GET /api/public/v2/datasets/{name}`,
    /// then every page of `GET /api/public/dataset-items` (in pages of
    /// [`fetch_items_page_size`](GetDatasetOptions::fetch_items_page_size),
    /// at [`version`](GetDatasetOptions::version) if set).
    ///
    /// Run an experiment on the result with `DatasetWithItems::run_experiment`
    /// (feature `otel`).
    ///
    /// ```rust,no_run
    /// use langfuse_rust_sdk::api::datasets::GetDatasetOptions;
    /// # async fn example(client: &langfuse_rust_sdk::LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
    /// let dataset = client.get_dataset("capital-cities", &GetDatasetOptions::default()).await?;
    /// for item in &dataset.items {
    ///     println!("{:?} -> {:?}", item.input, item.expected_output);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_dataset(
        &self,
        name: &str,
        options: &GetDatasetOptions,
    ) -> Result<DatasetWithItems, LangfuseError> {
        if options.fetch_items_page_size < 1 {
            return Err(LangfuseError::Config(
                "fetch_items_page_size must be at least 1".into(),
            ));
        }
        let dataset = self.datasets.get(name).await?;
        let params = ListDatasetItemsParams {
            dataset_name: Some(name.to_owned()),
            version: options.version.map(python_isoformat),
            limit: Some(options.fetch_items_page_size),
            ..Default::default()
        };
        let items: Vec<DatasetItem> = self
            .dataset_items
            .list_pages(params)
            .items()
            .try_collect()
            .await?;
        Ok(DatasetWithItems::new(dataset, items, options.version))
    }

    /// Create (or upsert by `id`) a dataset item like
    /// [`dataset_items.create`](crate::api::dataset_items::DatasetItemsClient::create),
    /// first uploading the media in `input`, `expected_output` and `metadata`.
    ///
    /// Media is found as in traced data (see [`media`](crate::media)): base64
    /// data URIs, which is how [`LangfuseMedia`] serializes, and the base64
    /// shapes of common LLM APIs. Each is replaced by its reference string and
    /// uploaded, linked to the item field where it first appears. With media,
    /// a missing `id` is generated (the uploads need it) and the dataset is
    /// fetched once for its ID. A failed upload returns the error and the
    /// item is not created.
    ///
    /// ```rust,no_run
    /// use langfuse_rust_sdk::media::LangfuseMedia;
    /// use langfuse_rust_sdk::models::dataset_items::CreateDatasetItemRequest;
    /// use langfuse_rust_sdk::models::media::MediaContentType;
    /// use serde_json::json;
    /// # async fn example(client: &langfuse_rust_sdk::LangfuseClient) -> Result<(), langfuse_rust_sdk::LangfuseError> {
    /// let chart = LangfuseMedia::from_file("chart.png", MediaContentType::ImagePng)?;
    /// let item = client
    ///     .create_dataset_item(&CreateDatasetItemRequest {
    ///         input: Some(json!({"question": "What does the chart show?", "image": chart})),
    ///         expected_output: Some(json!("Revenue doubled")),
    ///         ..CreateDatasetItemRequest::new("charts")
    ///     })
    ///     .await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn create_dataset_item(
        &self,
        request: &CreateDatasetItemRequest,
    ) -> Result<DatasetItem, LangfuseError> {
        let mut request = request.clone();
        let mut pending: Vec<(LangfuseMedia, MediaField)> = Vec::new();
        for (field, value) in [
            (MediaField::Input, &mut request.input),
            (MediaField::ExpectedOutput, &mut request.expected_output),
            (MediaField::Metadata, &mut request.metadata),
        ] {
            let Some(data) = value.take() else {
                continue;
            };
            let mut found = Vec::new();
            *value = Some(extract_media(data, &mut found));
            for media in found {
                if !pending
                    .iter()
                    .any(|(m, _)| m.media_id() == media.media_id())
                {
                    pending.push((media, field));
                }
            }
        }
        if !pending.is_empty() {
            let item_id = request
                .id
                .get_or_insert_with(|| uuid::Uuid::new_v4().to_string())
                .clone();
            let dataset_id = self.datasets.get(&request.dataset_name).await?.id;
            for (media, field) in &pending {
                media
                    .upload(
                        self,
                        &MediaTarget::dataset_item(&dataset_id, &item_id, *field),
                    )
                    .await?;
            }
        }
        self.dataset_items.create(&request).await
    }
}
