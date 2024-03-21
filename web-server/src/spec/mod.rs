use reqwest::Url;
use serde::Deserialize;

use crate::err::{RequestError1, ResponseErrorItem};

#[derive(Debug, Clone, Deserialize)]
pub struct RecomposeJson {
    pub video_url: Option<String>,
    pub rules_url: Option<String>,
    pub dest_url: Option<String>,
}

impl TryFrom<RecomposeJson> for RecomposeApi {
    type Error = RequestError1;

    fn try_from(json: RecomposeJson) -> Result<Self, RequestError1> {
        let mut err_reasons = Vec::with_capacity(3);

        fn extract_uri(
            field_name: &'static str,
            op_str: &Option<String>,
            errors: &mut Vec<ResponseErrorItem>,
        ) -> Option<Url> {
            if let Some(ref v) = op_str {
                if let Ok(u) = Url::parse(v) {
                    Some(u)
                } else {
                    let err = ResponseErrorItem::title_source(
                        format!("field {field_name} is not a valid URI"),
                        format!("/{field_name}"),
                    );
                    errors.push(err);
                    None
                }
            } else {
                let err = ResponseErrorItem::title_source(
                    format!("missing field {field_name}"),
                    format!("/{field_name}"),
                );
                errors.push(err);
                None
            }
        }

        let video_url = extract_uri("video_url", &json.video_url, &mut err_reasons);
        let rules_url = extract_uri("rules_url", &json.rules_url, &mut err_reasons);
        let dest_url = extract_uri("dest_url", &json.dest_url, &mut err_reasons);

        if err_reasons.is_empty() {
            // these three field should be infallible, but give a graceful error just in case
            let video_url =
                video_url.ok_or_else(|| RequestError1::sys_str("internal assertion error"))?;
            let rules_url =
                rules_url.ok_or_else(|| RequestError1::sys_str("internal assertion error"))?;
            let dest_url =
                dest_url.ok_or_else(|| RequestError1::sys_str("internal assertion error"))?;

            Ok(RecomposeApi {
                video_url,
                rules_url,
                dest_url,
            })
        } else {
            Err(RequestError1::Validation(err_reasons))
        }
    }
}

pub struct RecomposeApi {
    pub video_url: Url,
    pub rules_url: Url,
    pub dest_url: Url,
}
