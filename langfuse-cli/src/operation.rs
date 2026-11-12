//! What every `langfuse api <resource> <action>` command is: static metadata
//! about the HTTP operation, and a typed clap struct that prepares the request.

use std::fmt::Display;
use std::io::Read;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use serde::Serialize;
use serde_json::Value;

use crate::args::AllPages;
use crate::error::{CliError, Result};

/// HTTP method of an operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpMethod {
    /// The method name as sent on the wire.
    pub const fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
        }
    }
}

/// How a list operation pages through its results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pagination {
    /// `page` / `limit`, `meta.totalPages`.
    Page,
    /// `cursor` / `limit`, `meta.cursor`.
    Cursor,
}

impl Pagination {
    /// The name used in `api schema --json`.
    pub const fn as_str(self) -> &'static str {
        match self {
            Pagination::Page => "page",
            Pagination::Cursor => "cursor",
        }
    }
}

/// What a successful response carries, per the OpenAPI spec.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResponseKind {
    /// A JSON body.
    Json,
    /// No body (204).
    Empty,
}

/// How the request body is given.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodyKind {
    /// No request body.
    None,
    /// Field flags or `--body-json` / `--body-file`.
    Fields,
    /// Field flags selected by a discriminator flag, or `--body-json` / `--body-file`.
    Union {
        /// The discriminator flag (without `--`).
        discriminator: &'static str,
    },
    /// Only `--body-json` / `--body-file`.
    JsonOnly,
}

/// Static description of one API operation.
#[derive(Debug, Clone, Copy)]
pub struct OperationMeta {
    /// Canonical resource name (`prompts`).
    pub resource: &'static str,
    /// Canonical action name (`list`).
    pub action: &'static str,
    /// OpenAPI operationId.
    pub operation_id: &'static str,
    /// HTTP method.
    pub method: HttpMethod,
    /// Path template, e.g. `/api/public/v2/prompts/{promptName}`.
    pub path: &'static str,
    /// Pagination style of list operations.
    pub pagination: Option<Pagination>,
    /// Whether the operation needs Basic auth (all but `health get`).
    pub auth: bool,
    /// Successful response kind.
    pub response: ResponseKind,
    /// How the request body is given.
    pub body: BodyKind,
    /// Extra request headers.
    pub headers: &'static [(&'static str, &'static str)],
    /// Default columns of `--format table`.
    pub columns: &'static [&'static str],
}

impl OperationMeta {
    /// An authenticated operation with a JSON response and no body.
    pub const fn new(
        resource: &'static str,
        action: &'static str,
        operation_id: &'static str,
        method: HttpMethod,
        path: &'static str,
    ) -> Self {
        Self {
            resource,
            action,
            operation_id,
            method,
            path,
            pagination: None,
            auth: true,
            response: ResponseKind::Json,
            body: BodyKind::None,
            headers: &[],
            columns: &[],
        }
    }

    /// Page-based list.
    pub const fn page(mut self) -> Self {
        self.pagination = Some(Pagination::Page);
        self
    }

    /// Cursor-based list.
    pub const fn cursor(mut self) -> Self {
        self.pagination = Some(Pagination::Cursor);
        self
    }

    /// No authentication.
    pub const fn no_auth(mut self) -> Self {
        self.auth = false;
        self
    }

    /// 204 response.
    ///
    /// The executor has no call for a `PUT` without a response body, so
    /// declaring one does not compile (every `META` is a constant).
    pub const fn empty(mut self) -> Self {
        assert!(
            !matches!(self.method, HttpMethod::Put),
            "a PUT operation needs a JSON response"
        );
        self.response = ResponseKind::Empty;
        self
    }

    /// Body from field flags or the JSON channel.
    pub const fn fields(mut self) -> Self {
        self.body = BodyKind::Fields;
        self
    }

    /// Union body selected by `--<discriminator>`.
    pub const fn union(mut self, discriminator: &'static str) -> Self {
        self.body = BodyKind::Union { discriminator };
        self
    }

    /// Body only through `--body-json` / `--body-file`.
    pub const fn json_only(mut self) -> Self {
        self.body = BodyKind::JsonOnly;
        self
    }

    /// Extra request headers.
    pub const fn headers(mut self, headers: &'static [(&'static str, &'static str)]) -> Self {
        self.headers = headers;
        self
    }

    /// Default table columns.
    pub const fn columns(mut self, columns: &'static [&'static str]) -> Self {
        self.columns = columns;
        self
    }

    /// Whether the path template has the parameter `{name}`. Usable in
    /// constants: the operation macros assert it for every parameter they fill.
    pub const fn has_path_parameter(&self, name: &str) -> bool {
        let path = self.path.as_bytes();
        let name = name.as_bytes();
        let mut start = 0;
        while start + name.len() + 2 <= path.len() {
            if path[start] == b'{' && path[start + name.len() + 1] == b'}' {
                let mut matched = 0;
                while matched < name.len() && path[start + 1 + matched] == name[matched] {
                    matched += 1;
                }
                if matched == name.len() {
                    return true;
                }
            }
            start += 1;
        }
        false
    }

    /// The path parameter names, in path order.
    pub fn path_parameters(&self) -> Vec<&'static str> {
        let mut names = Vec::new();
        let mut rest = self.path;
        while let Some(start) = rest.find('{') {
            let Some(end) = rest[start..].find('}') else {
                break;
            };
            names.push(&rest[start + 1..start + end]);
            rest = &rest[start + end + 1..];
        }
        names
    }
}

/// Inputs an operation may read while preparing its request.
pub struct Context<'a> {
    /// Standard input, for `--body-file -`.
    pub stdin: &'a mut dyn Read,
}

impl std::fmt::Debug for Context<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Context").finish_non_exhaustive()
    }
}

/// A typed `langfuse api` action.
pub trait Operation {
    /// The operation it calls.
    const META: OperationMeta;

    /// Build the request from the parsed arguments.
    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest>;

    /// `--all` / `--max-items`, for paginated lists.
    fn all_pages(&self) -> Option<&AllPages> {
        None
    }
}

/// Object-safe view of an [`Operation`], used for dispatch.
pub trait ApiOperation {
    /// The operation's metadata.
    fn meta(&self) -> &'static OperationMeta;
    /// Build the request.
    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest>;
    /// `--all` / `--max-items`.
    fn all_pages(&self) -> Option<&AllPages>;
}

impl<T: Operation> ApiOperation for T {
    fn meta(&self) -> &'static OperationMeta {
        &T::META
    }

    fn prepare(&self, ctx: &mut Context<'_>) -> Result<PreparedRequest> {
        Operation::prepare(self, ctx)
    }

    fn all_pages(&self) -> Option<&AllPages> {
        Operation::all_pages(self)
    }
}

/// Characters left unencoded in a path segment: the RFC 3986 unreserved set,
/// as the SDK's `encode_path_segment` (`folder/name` → `folder%2Fname`).
const PATH_SEGMENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'.')
    .remove(b'_')
    .remove(b'~');

/// Percent-encode one path segment as the SDK does.
pub fn encode_segment(value: &str) -> String {
    utf8_percent_encode(value, PATH_SEGMENT).to_string()
}

/// A fully prepared request; `--curl` renders it and the executor sends it.
#[derive(Debug, Clone)]
pub struct PreparedRequest {
    /// The operation.
    pub meta: &'static OperationMeta,
    /// The path with its parameters encoded.
    pub path: String,
    /// Query parameters in order; names may repeat.
    pub query: Vec<(String, String)>,
    /// JSON body.
    pub body: Option<Value>,
}

impl PreparedRequest {
    /// A request for `meta` with the path template still unfilled.
    pub fn new(meta: &'static OperationMeta) -> Self {
        Self {
            meta,
            path: meta.path.to_owned(),
            query: Vec::new(),
            body: None,
        }
    }

    /// Fill the path parameter `{name}` with an encoded `value`.
    pub fn path_param(mut self, name: &str, value: impl Display) -> Self {
        let placeholder = format!("{{{name}}}");
        debug_assert!(
            self.path.contains(&placeholder),
            "{} has no path parameter {name}",
            self.meta.operation_id
        );
        let encoded = encode_segment(&value.to_string());
        self.path = self.path.replace(&placeholder, &encoded);
        self
    }

    /// Append the query parameters `params` serializes to (the SDK's params
    /// structs), with the encoding the SDK uses.
    pub fn query<Q: Serialize>(mut self, params: &Q) -> Result<Self> {
        let encoded = serde_urlencoded::to_string(params)
            .map_err(|error| CliError::usage(format!("Invalid query parameters: {error}")))?;
        self.query
            .extend(url::form_urlencoded::parse(encoded.as_bytes()).into_owned());
        Ok(self)
    }

    /// Append one query parameter if `value` is set.
    pub fn query_opt(mut self, name: &str, value: Option<impl Display>) -> Self {
        if let Some(value) = value {
            self.query.push((name.to_owned(), value.to_string()));
        }
        self
    }

    /// Set the JSON body.
    pub fn body(mut self, body: Value) -> Self {
        self.body = Some(body);
        self
    }

    /// Set the JSON body from a serializable value (the SDK's request types).
    pub fn body_from<B: Serialize>(self, body: &B) -> Result<Self> {
        let value = serde_json::to_value(body)
            .map_err(|error| CliError::usage(format!("Invalid request body: {error}")))?;
        Ok(self.body(value))
    }

    /// The value of the query parameter `name` (the last one if repeated).
    pub fn query_value(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Set the query parameter `name`, replacing earlier ones.
    pub fn set_query(&mut self, name: &str, value: impl Into<String>) {
        self.query.retain(|(key, _)| key != name);
        self.query.push((name.to_owned(), value.into()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const META: OperationMeta = OperationMeta::new(
        "skills",
        "update-version",
        "skills_setVersionLabels",
        HttpMethod::Patch,
        "/api/public/skills/{skillName}/versions/{skillVersion}",
    );

    // Checked when this module is compiled, like the ones in the macros.
    const _: () = {
        assert!(META.has_path_parameter("skillName"));
        assert!(META.has_path_parameter("skillVersion"));
        assert!(!META.has_path_parameter("skill"));
        assert!(!META.has_path_parameter("skillVersions"));
        assert!(!META.has_path_parameter(""));
    };

    #[test]
    fn path_parameters_are_listed_in_order_and_encoded() {
        assert_eq!(META.path_parameters(), ["skillName", "skillVersion"]);
        let request = PreparedRequest::new(&META)
            .path_param("skillName", "folder/name with space")
            .path_param("skillVersion", 3);
        assert_eq!(
            request.path,
            "/api/public/skills/folder%2Fname%20with%20space/versions/3"
        );
    }

    #[test]
    fn query_keeps_repeated_names_and_order() {
        #[derive(Serialize)]
        struct Params {
            limit: Option<i64>,
            name: &'static str,
        }
        let mut request = PreparedRequest::new(&META)
            .query(&Params {
                limit: Some(5),
                name: "a b",
            })
            .unwrap()
            .query_opt("environment", Some("prod"))
            .query_opt("environment", Some("dev"))
            .query_opt("unset", None::<&str>);
        assert_eq!(
            request.query,
            [
                ("limit".to_owned(), "5".to_owned()),
                ("name".to_owned(), "a b".to_owned()),
                ("environment".to_owned(), "prod".to_owned()),
                ("environment".to_owned(), "dev".to_owned()),
            ]
        );
        request.set_query("limit", "10");
        assert_eq!(request.query_value("limit"), Some("10"));
        assert_eq!(request.query_value("environment"), Some("dev"));
    }
}
