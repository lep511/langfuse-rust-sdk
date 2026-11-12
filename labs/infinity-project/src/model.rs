//! The model the chat talks to: one of those its provider offers, the one
//! `/model` chooses.

use std::cell::Cell;
use std::fmt;
use std::rc::Rc;
use std::sync::Arc;

use async_trait::async_trait;
use infinity_agent_core::system::{ModelSource, ResolvedModel};
use infinity_provider_protocol::{ModelEntry, ModelProvider};
use rap_protocol::ThreadId;

use crate::menu::Value;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// The provider's models, and the one in use.
///
/// Its clones share the choice. The agent system asks one of them for the
/// model at the start of each round of a turn, so the model chosen through
/// another is the model of the next round, in the same conversation.
#[derive(Clone)]
pub struct ModelChoice(Rc<Inner>);

struct Inner {
    provider: Arc<dyn ModelProvider>,
    /// The models the provider offers, in its order.
    models: Vec<ModelEntry>,
    /// Where the one in use is among them.
    current: Cell<usize>,
}

impl ModelChoice {
    /// The models of `provider`, with the one that has `model_id` in use.
    pub async fn new(provider: Arc<dyn ModelProvider>, model_id: &str) -> Result<Self, BoxError> {
        let models = provider.list_models().await?;
        let current = models
            .iter()
            .position(|model| model.model_id == model_id)
            .ok_or_else(|| format!("the provider has no model {model_id}"))?;
        Ok(Self(Rc::new(Inner {
            provider,
            models,
            current: Cell::new(current),
        })))
    }

    /// The model in use.
    pub fn current(&self) -> &ModelEntry {
        &self.0.models[self.0.current.get()]
    }

    /// Puts in use the model called `name`: a model's name or its ID, or
    /// text that only one model has in either. All of it in any case.
    pub fn select(&self, name: &str) -> Result<&ModelEntry, SelectError> {
        let models = &self.0.models;
        let wanted = name.to_lowercase();
        let names = |model: &ModelEntry| {
            [&model.display_name, &model.model_id].map(|name| name.to_lowercase())
        };
        // A whole name first: the name of a model can be in another's.
        let index = match models
            .iter()
            .position(|model| names(model).contains(&wanted))
        {
            Some(index) => index,
            None => {
                let has =
                    |model: &ModelEntry| names(model).iter().any(|name| name.contains(&wanted));
                let like: Vec<usize> = (0..models.len())
                    .filter(|&index| has(&models[index]))
                    .collect();
                match like[..] {
                    [index] => index,
                    [] => return Err(SelectError::Unknown(name.to_owned())),
                    _ => {
                        let like = like.iter().map(|&index| models[index].display_name.clone());
                        return Err(SelectError::Ambiguous(name.to_owned(), like.collect()));
                    }
                }
            }
        };
        self.0.current.set(index);
        Ok(&models[index])
    }

    /// The models as a menu lists them: by name, with the size of their
    /// context window, and the one in use marked.
    pub fn values(&self) -> Vec<Value> {
        let value = |(index, model): (usize, &ModelEntry)| {
            let context = match model.context_window {
                // The provider does not say.
                0 => None,
                tokens => Some(format!("{} context", short_count(tokens))),
            };
            let current = index == self.0.current.get();
            let marked = current.then(|| "current".to_owned());
            let about: Vec<String> = context.into_iter().chain(marked).collect();
            Value {
                name: model.display_name.clone(),
                about: about.join(" · "),
                current,
            }
        };
        self.0.models.iter().enumerate().map(value).collect()
    }
}

/// `1M` for a million and `200k` for two hundred thousand.
fn short_count(count: usize) -> String {
    match count {
        millions if millions % 1_000_000 == 0 => format!("{}M", millions / 1_000_000),
        thousands if thousands % 1_000 == 0 => format!("{}k", thousands / 1_000),
        count => count.to_string(),
    }
}

#[async_trait(?Send)]
impl ModelSource for ModelChoice {
    async fn resolve(&self, _thread_id: &ThreadId<str>) -> Result<ResolvedModel, BoxError> {
        let model = self.current();
        Ok(ResolvedModel {
            provider: self.0.provider.clone(),
            model_id: model.model_id.clone(),
            context_window: model.context_window,
            supports_image_input: model.supports_image_input,
        })
    }
}

/// Why a name chose no model.
#[derive(Debug, PartialEq, Eq)]
pub enum SelectError {
    /// No model has the name, or has it in its own.
    Unknown(String),
    /// These models all have it in theirs.
    Ambiguous(String, Vec<String>),
}

impl fmt::Display for SelectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unknown(name) => {
                write!(f, "{name} is not a model. /model lists the models.")
            }
            Self::Ambiguous(name, models) => {
                let models = models.join(", ");
                write!(f, "{name} is in the name of several models: {models}.")
            }
        }
    }
}

impl std::error::Error for SelectError {}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use infinity_agent_core::system::ModelSource;
    use infinity_provider_protocol::{
        CompletionError, CompletionRequest, ErrorClass, ModelEntry, ModelProvider, ModelStream,
    };
    use rap_protocol::ThreadId;

    use super::{BoxError, ModelChoice, SelectError, short_count};

    /// A provider with a catalog and no model behind it.
    struct Catalog;

    #[async_trait]
    impl ModelProvider for Catalog {
        async fn list_models(&self) -> Result<Vec<ModelEntry>, BoxError> {
            let model =
                |name: &str, model_id: &str, context_window, supports_image_input| ModelEntry {
                    model_id: model_id.to_owned(),
                    display_name: name.to_owned(),
                    context_window,
                    max_output_tokens: None,
                    supports_image_input,
                };
            Ok(vec![
                model(
                    "claude-fable-5.1",
                    "global.anthropic.claude-fable-5-1",
                    1_000_000,
                    true,
                ),
                model(
                    "claude-fable-5",
                    "global.anthropic.claude-fable-5",
                    1_000_000,
                    true,
                ),
                model(
                    "claude-opus-4-6 1m",
                    "global.anthropic.claude-opus-4-6-v1:1m",
                    1_000_000,
                    true,
                ),
                model(
                    "claude-opus-4-6",
                    "global.anthropic.claude-opus-4-6-v1",
                    200_000,
                    true,
                ),
                model("nova", "amazon.nova-pro-v1:0", 0, false),
            ])
        }

        async fn invoke_model(
            &self,
            _model_id: &str,
            _request: CompletionRequest,
        ) -> Result<ModelStream, CompletionError> {
            Err(CompletionError::provider(ErrorClass::Fatal, "a catalog"))
        }
    }

    async fn choice() -> ModelChoice {
        let model_id = "global.anthropic.claude-opus-4-6-v1";
        ModelChoice::new(Arc::new(Catalog), model_id).await.unwrap()
    }

    #[tokio::test]
    async fn the_chat_starts_on_a_model_of_the_provider() {
        assert_eq!(choice().await.current().display_name, "claude-opus-4-6");
        let Err(error) = ModelChoice::new(Arc::new(Catalog), "gpt").await else {
            panic!("gpt is not in the catalog");
        };
        assert_eq!(error.to_string(), "the provider has no model gpt");
    }

    #[tokio::test]
    async fn a_model_is_chosen_by_name_or_by_id() {
        let choice = choice().await;
        let chosen = |name| choice.select(name).map(|model| model.model_id.as_str());
        assert_eq!(
            chosen("claude-fable-5"),
            Ok("global.anthropic.claude-fable-5")
        );
        assert_eq!(choice.current().display_name, "claude-fable-5");
        assert_eq!(
            chosen("Claude-Opus-4-6 1M"),
            Ok("global.anthropic.claude-opus-4-6-v1:1m")
        );
        assert_eq!(chosen("amazon.nova-pro-v1:0"), Ok("amazon.nova-pro-v1:0"));
        assert_eq!(choice.current().display_name, "nova");
    }

    #[tokio::test]
    async fn a_model_is_chosen_by_text_that_only_it_has() {
        let choice = choice().await;
        let chosen = |name| choice.select(name).map(|model| model.display_name.as_str());
        assert_eq!(chosen("5.1"), Ok("claude-fable-5.1"));
        assert_eq!(chosen("NOVA-PRO"), Ok("nova"));
        assert_eq!(chosen(":1m"), Ok("claude-opus-4-6 1m"));
        // A whole name wins over the names it is in.
        assert_eq!(chosen("claude-opus-4-6"), Ok("claude-opus-4-6"));
    }

    #[tokio::test]
    async fn a_name_that_chooses_no_model_leaves_the_one_in_use() {
        let choice = choice().await;
        let unknown = choice.select("gpt").unwrap_err();
        assert_eq!(unknown, SelectError::Unknown("gpt".to_owned()));
        assert_eq!(
            unknown.to_string(),
            "gpt is not a model. /model lists the models."
        );
        let ambiguous = choice.select("Fable").unwrap_err();
        assert_eq!(
            ambiguous.to_string(),
            "Fable is in the name of several models: claude-fable-5.1, claude-fable-5."
        );
        assert_eq!(choice.current().display_name, "claude-opus-4-6");
    }

    #[tokio::test]
    async fn the_next_round_is_on_the_model_chosen() {
        let choice = choice().await;
        // The agent system has a clone of its own.
        let source = choice.clone();
        let thread = ThreadId::from_ref("thread");
        let resolved = source.resolve(thread).await.unwrap();
        assert_eq!(resolved.model_id, "global.anthropic.claude-opus-4-6-v1");
        assert_eq!(resolved.context_window, 200_000);
        assert!(resolved.supports_image_input);

        choice.select("nova").unwrap();
        let resolved = source.resolve(thread).await.unwrap();
        assert_eq!(resolved.model_id, "amazon.nova-pro-v1:0");
        assert_eq!(resolved.context_window, 0);
        assert!(!resolved.supports_image_input);
    }

    #[tokio::test]
    async fn models_are_listed_with_their_context_and_the_one_in_use() {
        let choice = choice().await;
        choice.select("nova").unwrap();
        choice.select("fable-5.1").unwrap();
        let values = choice.values();
        let current: Vec<_> = values.iter().filter(|value| value.current).collect();
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].name, "claude-fable-5.1");
        let listed: Vec<_> = values
            .into_iter()
            .map(|value| (value.name, value.about))
            .collect();
        let row = |name: &str, about: &str| (name.to_owned(), about.to_owned());
        assert_eq!(
            listed,
            [
                row("claude-fable-5.1", "1M context · current"),
                row("claude-fable-5", "1M context"),
                row("claude-opus-4-6 1m", "1M context"),
                row("claude-opus-4-6", "200k context"),
                row("nova", ""),
            ]
        );
    }

    #[test]
    fn counts_are_short() {
        assert_eq!(short_count(1_000_000), "1M");
        assert_eq!(short_count(200_000), "200k");
        assert_eq!(short_count(1_500_000), "1500k");
        assert_eq!(short_count(4_096), "4096");
    }
}
