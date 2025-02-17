use crate::{contact::UserInfo, DingTalk};
use deadpool_redis::redis::cmd;
use deadpool_redis::Pool;

use log::{error, info, warn};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Serialize, Deserialize, Debug)]
pub struct Organization {
    #[serde(rename = "licenseUrl")]
    pub license_url: String,

    #[serde(rename = "orgName")]
    pub name: String,

    #[serde(rename = "registrationNum")]
    pub registration_no: String,

    #[serde(rename = "unifiedSocialCredit")]
    pub unified_social_credit: String,

    #[serde(rename = "organizationCode")]
    pub organization_code: String,

    #[serde(rename = "legalPerson")]
    pub legal_person: String,

    #[serde(rename = "licenseOrgName")]
    pub license_org_name: String,

    #[serde(rename = "authLevel")]
    pub auth_level: i32,
}

impl DingTalk {
    /// Creates a new `OrgApp` instance with the given corporate ID and configuration.
    ///
    /// This method creates a new instance of `OrgApp` with the given corporate ID and the same
    /// configuration as the current `DingTalk` instance. This is useful for accessing the DingTalk
    /// API of a specific organization.
    ///
    /// # Arguments
    ///
    /// * `corp_id` - The corporate ID of the organization to create the `OrgApp` for.
    ///
    /// # Returns
    ///
    /// A new `OrgApp` instance with the given corporate ID and configuration.
    pub fn set_corp_id(&self, corp_id: String) -> OrgApp {
        OrgApp::new(
            self.appid.clone(),
            self.app_secret.clone(),
            corp_id,
            self.rdb.clone(),
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserGetByCodeResponse {
    device_id: String,
    #[serde(rename = "name")]
    pub username: String,
    #[serde(rename = "sys")]
    is_admin: bool,
    #[serde(rename = "sys_level")]
    level: i32, //1: 主管理员 2:子管理员 100:老板 0:其他
    #[serde(rename = "unionid")]
    pub union_id: String,
    #[serde(rename = "userid")]
    pub user_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct Department {
    #[serde(rename = "dept_id")]
    id: i32,
    #[serde(rename = "order")]
    sort_id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
struct LeaderInDepartment {
    #[serde(rename = "dept_id")]
    id: i32,
    leader: bool,
}

#[derive(Serialize, Deserialize, Debug)]
struct Role {
    id: i32,
    name: String,
    group_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
struct UserGetProfileResponse {
    active: bool,
    admin: bool,
    avatar: String,
    boss: bool,
    create_time: String,
    dept_id_list: Vec<i32>,
    dept_order_list: Vec<Department>,
    email: String,
    exclusive_account: bool,
    hide_mobile: bool,
    job_number: String,
    leader_in_dept: Vec<LeaderInDepartment>,
    mobile: String,
    #[serde(rename = "name")]
    username: String,
    org_email: String,
    real_authed: bool,
    remark: String,
    role_list: Vec<Role>,
    senior: bool,
    state_code: String,
    telephone: String,
    title: String,
    #[serde(default)]
    union_emp_ext: HashMap<String, String>,
    #[serde(rename = "unionid")]
    union_id: String,
    #[serde(rename = "userid")]
    user_id: String,
    work_place: String,
}

pub struct OrgApp {
    appid: String,
    app_secret: String,
    corp_id: String,
    client: reqwest::Client,
    rdb: Arc<Pool>,
}

impl OrgApp {
    pub fn new(appid: String, app_secret: String, corp_id: String, rdb: Arc<Pool>) -> OrgApp {
        OrgApp {
            appid,
            app_secret,
            corp_id,
            rdb,
            client: reqwest::Client::new(),
        }
    }

    async fn get_access_token(&self) -> Result<String, Box<dyn std::error::Error>> {
        #[derive(Serialize, Deserialize, Debug)]
        struct AccessToken {
            access_token: String,
            #[serde(rename = "expires_in")]
            expire_in: i32,
        }

        let mut rdb = self.rdb.get().await.unwrap();
        let value: Option<String> = cmd("GET")
            .arg(&self.corp_id)
            .query_async(&mut rdb)
            .await
            .unwrap_or(None);

        if let Some(bytes) = value {
            return Ok(bytes);
        }

        let mut params = HashMap::new();
        params.insert("client_id", self.appid.clone());
        params.insert("client_secret", self.app_secret.clone());
        params.insert("grant_type", "client_credentials".to_string());

        let response = self
            .client
            .post(format!(
                "https://api.dingtalk.com/v1.0/oauth2/{}/token",
                self.corp_id
            ))
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to get organization access token: {}",
                response.status()
            )
            .into());
        }

        let result = response.json::<AccessToken>().await?;
        warn!("fetch_org_access_token result: {:#?}", result);

        let mut rdb = self.rdb.get().await.unwrap();
        cmd("SETEX")
            .arg(&self.corp_id)
            .arg(7200)
            .arg(&result.access_token)
            .query_async::<()>(&mut rdb)
            .await
            .unwrap();

        Ok(result.access_token)
    }

    /// Retrieves the organization information associated with the provided corporate ID.
    ///
    /// [Documents](https://open.dingtalk.com/document/orgapp/obtain-enterprise-authentication-information)
    ///
    /// This function first obtains an access token for the organization, then makes a request to the
    /// DingTalk API to retrieve the organization details.
    ///
    /// # Arguments
    ///
    /// * `&self` - The `OrgApp` instance to use for the request.
    ///
    /// # Returns
    ///
    /// A `Result` containing an `Organization` struct with the organization details if successful,
    /// otherwise an error string.
    pub async fn get_organization(&self) -> Result<Organization, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        match self.get_access_token().await {
            Ok(at) => {
                headers.insert(
                    HeaderName::from_static("x-acs-dingtalk-access-token"),
                    HeaderValue::from_str(&at).unwrap(),
                );
            }
            Err(e) => return Err(e),
        };

        let url: String = format!(
            "https://api.dingtalk.com/v1.0/contact/organizations/authInfos?targetCorpId={}",
            self.corp_id
        );
        let response = self.client.get(&url).headers(headers).send().await?;

        if !response.status().is_success() {
            return Err(format!("Failed to get organization: {}", response.status()).into());
        }

        let result = response.json::<Organization>().await?;
        info!("get_organization: {:?}", result);

        Ok(result)
    }

    /// Retrieves the user ID associated with the given authorization code.
    ///
    /// This asynchronous function sends a POST request to the DingTalk API to fetch
    /// user details associated with the provided `code`. The request includes an
    /// access token in the query parameters for authentication.
    ///
    /// # Arguments
    ///
    /// * `code` - A string representing the authorization code to fetch user information.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the user ID as a `String` if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the response status is not successful, or if the request fails.
    async fn get_user_id(&self, code: String) -> Result<String, Box<dyn std::error::Error>> {
        let token = match self.get_access_token().await {
            Ok(value) => value,
            Err(e) => return Err(e),
        };

        let mut params = HashMap::new();
        params.insert("code", code);

        let response = self
            .client
            .post(format!(
                "https://oapi.dingtalk.com/topapi/v2/user/getuserinfo?access_token={}",
                token
            ))
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!("Failed to response user info: {}", response.status()).into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            errcode: i32,
            errmsg: String,
            result: UserGetByCodeResponse,
            request_id: Option<String>,
        }
        let user = match response.json::<Response>().await {
            Ok(value) => value.result,
            Err(e) => {
                error!("response get_user info {:?}", e);
                return Err(e.into());
            }
        };

        info!("get_org_user_id {:?}", &user);

        Ok(user.user_id)
    }

    /// Retrieves user information from DingTalk using the provided code.
    ///
    /// [Documents](https://open.dingtalk.com/document/orgapp/get-user-info-by-code)
    ///
    /// This asynchronous function sends a POST request to the DingTalk API to fetch
    /// user details associated with the provided `code`. The request includes an
    /// access token in the query parameters for authentication.
    ///
    /// # Arguments
    ///
    /// * `code` - A string representing the authorization code to fetch user information.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a `UserInfo` object if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    ///
    /// # Errors
    ///
    pub async fn get_userinfo(&self, code: String) -> Result<UserInfo, Box<dyn std::error::Error>> {
        let mut params = HashMap::new();
        match self.get_user_id(code.clone()).await {
            Ok(id) => params.insert("userid", id),
            Err(e) => return Err(e),
        };

        let at = match self.get_access_token().await {
            Ok(at) => at,
            Err(e) => return Err(e),
        };

        let response = self
            .client
            .post(format!(
                "https://oapi.dingtalk.com/topapi/v2/user/get?access_token={}",
                at
            ))
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to response get org user info: {}",
                response.status()
            )
            .into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            errcode: i32,
            errmsg: String,
            result: UserGetProfileResponse,
            request_id: Option<String>,
        }
        let profile = match response.json::<Response>().await {
            Ok(res) => res.result,
            Err(e) => {
                error!("response get org user info {:?}", e);
                return Err(e.into());
            }
        };
        info!("get org user info {:?}", &profile);

        let profile: UserInfo = UserInfo {
            email: Some(profile.org_email.clone()),
            union_id: profile.union_id.clone(),
            username: profile.username.clone(),
            visitor: None,
            mobile: Some(profile.mobile.clone()),
            open_id: None,
            state_code: "".to_string(),
        };

        Ok(profile)
    }
}
