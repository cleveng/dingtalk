use crate::DingTalk;
use log::info;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct UserInfo {
    pub email: Option<String>,
    pub mobile: Option<String>,
    #[serde(rename = "nick")]
    pub username: String,
    #[serde(default, rename = "openId")]
    pub open_id: Option<String>,
    #[serde(rename = "unionId")]
    pub union_id: String,
    #[serde(rename = "stateCode")]
    pub state_code: String,
    pub visitor: Option<bool>,
}

impl DingTalk {
    /// Get the user info of the given union ID.
    ///
    /// [Documents](https://open.dingtalk.com/document/isvapp/get-user-info)
    ///
    /// # Arguments
    ///
    /// * `union_id` - The union ID of the DingTalk user.
    ///
    /// # Returns
    ///
    /// A `Result` containing the user info if successful, otherwise an error string.
    pub async fn get_contact_userinfo(
        &self,
        union_id: String,
    ) -> Result<UserInfo, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        match self.get_app_access_token().await {
            Ok(at) => headers.insert(
                HeaderName::from_static("x-acs-dingtalk-access-token"),
                HeaderValue::from_str(&at).unwrap(),
            ),
            Err(e) => return Err(e),
        };

        let url: String = format!("https://api.dingtalk.com/v1.0/contact/users/{}", union_id);
        let response = self.client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get user info: {}", response.status()).into());
        }

        let result = response.json::<UserInfo>().await?;

        info!("union_id: {union_id}, fetch user info: {:#?}", &result);

        Ok(result)
    }
}
