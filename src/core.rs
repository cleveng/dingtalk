use std::collections::HashMap;

use crate::DingTalk;
use deadpool_redis::redis::cmd;
use serde::{Deserialize, Serialize};
use url::{form_urlencoded, Url};

impl DingTalk {
    /// Generate the redirect URL for DingTalk authorization.
    ///
    /// [Documents](https://open.dingtalk.com/document/isvapp/obtain-identity-credentials)
    ///
    /// # Arguments
    ///
    /// * `redirect_uri` - The redirect URI after authorization.
    /// * `state` - An optional state string, which is used to prevent CSRF attacks.
    ///
    /// # Returns
    ///
    /// The redirect URL as a string.
    pub fn get_redirect_url(&self, redirect_uri: String, state: Option<String>) -> String {
        let mut url = Url::parse("https://login.dingtalk.com/oauth2/auth").unwrap();

        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("redirect_uri", &redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("client_id", self.appid.as_ref())
            .append_pair("scope", "openid corpid")
            .append_pair("state", state.unwrap_or("".to_string()).as_ref())
            .append_pair("prompt", "consent")
            .finish();

        url.set_query(Some(&query));

        url.to_string()
    }

    /// Obtain the access token for the application.
    ///
    /// This asynchronous function sends a POST request to the DingTalk API to obtain the access token
    /// for the application. The request includes the authorization code in the query parameters for
    /// authentication.
    ///
    /// [Documents](https://open.dingtalk.com/document/isvapp/obtain-identity-credentials)
    ///
    /// # Arguments
    ///
    /// * `code` - The authorization code to obtain the access token.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the access token as a string if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the response status is not successful, or if the request fails.
    pub async fn set_app_access_token(
        &self,
        code: String,
    ) -> Result<String, Box<dyn std::error::Error>> {
        let mut params = HashMap::new();
        params.insert("clientId", self.appid.clone());
        params.insert("clientSecret", self.app_secret.clone());
        params.insert("code", code.clone());
        params.insert("refreshToken", "".to_string());
        params.insert("grantType", "authorization_code".to_string());

        let response = self
            .client
            .post("https://api.dingtalk.com/v1.0/oauth2/userAccessToken")
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get access token: {}", response.status()).into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct AccessToken {
            #[serde(rename = "accessToken")]
            pub access_token: String,
            #[serde(rename = "refreshToken")]
            pub refresh_token: String,
            #[serde(rename = "corpId")]
            pub corp_id: String,
            #[serde(rename = "expireIn")]
            pub expire_in: i32,
        }
        let at = response.json::<AccessToken>().await?;

        let mut rdb = self.rdb.get().await.unwrap();
        cmd("SET")
            .arg(&self.appid)
            .arg(serde_json::to_string(&at)?)
            .query_async::<()>(&mut rdb)
            .await
            .unwrap();

        Ok(at.corp_id) // 企业corpId
    }

    /// Get the access token for the application.
    ///
    /// The access token is stored in Redis by calling [set_app_access_token].
    ///
    /// # Returns
    ///
    /// A Result containing the access token as a string if the access token exists, otherwise an error string.
    pub async fn get_app_access_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        let mut rdb = self.rdb.get().await.unwrap();
        let value: Option<String> = cmd("GET")
            .arg(&self.appid)
            .query_async(&mut rdb)
            .await
            .unwrap_or(None);

        #[derive(Serialize, Deserialize, Debug)]
        struct AccessToken {
            #[serde(rename = "accessToken")]
            pub access_token: String,
            #[serde(rename = "refreshToken")]
            pub refresh_token: String,
            #[serde(rename = "corpId")]
            pub corp_id: String,
            #[serde(rename = "expireIn")]
            pub expire_in: i32,
        }
        if let Some(bytes) = value {
            let value: AccessToken = serde_json::from_str(&bytes).unwrap();
            return Ok(value.access_token);
        }

        Err("Failed to get access token".into())
    }
}
