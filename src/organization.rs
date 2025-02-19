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
pub struct Department {
    #[serde(rename = "dept_id")]
    pub id: i32,
    #[serde(rename = "order")]
    pub sort_id: i64,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LeaderInDepartment {
    #[serde(rename = "dept_id")]
    pub id: i32,
    pub leader: bool,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Role {
    pub id: i32,
    pub name: String,
    pub group_name: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct UserGetProfileResponse {
    pub active: bool,
    pub admin: bool,
    pub avatar: String,
    pub boss: bool,
    pub create_time: String,
    pub dept_id_list: Vec<i32>,
    pub dept_order_list: Vec<Department>,
    #[serde(default)]
    pub email: Option<String>,
    pub exclusive_account: bool,
    pub hide_mobile: bool,
    #[serde(default)]
    pub job_number: String,
    pub leader_in_dept: Vec<LeaderInDepartment>,
    pub mobile: String,
    #[serde(rename = "name")]
    pub username: String,
    #[serde(default)]
    pub org_email: Option<String>,
    pub real_authed: bool,
    pub remark: String,
    pub role_list: Vec<Role>,
    pub senior: bool,
    pub state_code: String,
    pub telephone: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub union_emp_ext: HashMap<String, String>,
    #[serde(rename = "unionid")]
    pub union_id: String,
    #[serde(rename = "userid")]
    pub user_id: String,
    pub work_place: String,
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
            email: profile.org_email.clone(),
            union_id: profile.union_id.clone(),
            username: profile.username.clone(),
            visitor: None,
            mobile: Some(profile.mobile.clone()),
            open_id: None,
            state_code: "".to_string(),
        };

        Ok(profile)
    }

    /// Retrieves the total number of employees in the organization.
    ///
    /// [获取员工人数](https://open.dingtalk.com/document/orgapp/obtain-the-number-of-employees-v2)
    ///
    /// If `only_active` is `Some(true)`, only active employees are counted.
    ///
    /// # Arguments
    ///
    /// * `only_active` - An optional boolean indicating whether to only count active employees.
    ///
    /// # Returns
    ///
    /// A `Result` containing the total number of employees if successful, otherwise an error string.
    pub async fn get_employee_count(
        &self,
        only_active: Option<bool>,
    ) -> Result<i32, Box<dyn std::error::Error>> {
        let mut params = HashMap::new();
        params.insert("only_active", only_active.unwrap_or(false));

        let at = match self.get_access_token().await {
            Ok(at) => at,
            Err(e) => return Err(e),
        };

        let response = self
            .client
            .post(format!(
                "https://oapi.dingtalk.com/topapi/user/count?access_token={}",
                at
            ))
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to response get employee count: {}",
                response.status()
            )
            .into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            errcode: i32,
            errmsg: String,
            result: CountUserResponse,
            request_id: Option<String>,
        }

        let res = response.json::<Response>().await?;

        Ok(res.result.count)
    }

    /// Query employees on job.
    ///
    /// [获取在职员工列表](https://open.dingtalk.com/document/orgapp/intelligent-personnel-query-the-list-of-on-the-job-employees-of-the)
    ///
    /// # Arguments
    ///
    /// * `status` - A string array representing the status of the employees to query.
    /// * `offset` - An integer representing the offset of the query.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `PageResult` object if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    pub async fn query_on_job_employees(
        &self,
        status: String,
        offset: i32,
    ) -> Result<PageResult, Box<dyn std::error::Error>> {
        let mut params: HashMap<&str, String> = HashMap::new();
        params.insert("status_list", status);
        params.insert("offset", format!("{}", offset));
        params.insert("size", "50".to_string());

        let at = match self.get_access_token().await {
            Ok(at) => at,
            Err(e) => return Err(e),
        };

        let response = self
            .client
            .post(format!(
                "https://oapi.dingtalk.com/topapi/smartwork/hrm/employee/queryonjob?access_token={}",
                at
            ))
            .json(&params)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(format!(
                "Failed to response get employee count: {}",
                response.status()
            )
            .into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            errcode: i32,
            errmsg: String,
            result: PageResult,
            request_id: Option<String>,
        }

        let res = response.json::<Response>().await?;

        Ok(res.result)
    }

    /// Retrieves a list of employees who are no longer on the job.
    ///
    /// [获取离职员工列表](https://open.dingtalk.com/document/orgapp/obtain-the-list-of-employees-who-have-left)
    ///
    /// The results are paginated, with the `offset` parameter specifying the starting
    /// index of the page. The `nextToken` parameter is used to fetch the next page.
    ///
    /// # Arguments
    ///
    /// * `offset` - The starting index of the page to fetch.
    ///
    /// # Returns
    ///
    /// A `Result` containing a `PageResult` object if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the response status is not successful, or if the request fails.
    pub async fn query_off_job_employees(
        &self,
        offset: i64,
    ) -> Result<PageResult, Box<dyn std::error::Error>> {
        let mut headers = HeaderMap::new();
        match self.get_access_token().await {
            Ok(at) => headers.insert(
                HeaderName::from_static("x-acs-dingtalk-access-token"),
                HeaderValue::from_str(&at).unwrap(),
            ),
            Err(e) => return Err(e),
        };

        let url: String = format!(
            "https://api.dingtalk.com/v1.0/hrm/employees/dismissions?nextToken={}&maxResults=50",
            offset
        );
        info!("query_off_job_employees: {}", url);

        let response = self.client.get(&url).headers(headers).send().await?;
        if !response.status().is_success() {
            return Err(format!("Failed to get user info: {}", response.status()).into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            #[serde(rename = "nextToken")]
            next_cursor: i64,
            #[serde(rename = "hasMore")]
            has_more: bool,
            #[serde(rename = "userIdList")]
            data: Vec<String>,
        }
        let result = response.json::<Response>().await?;
        info!("query_off_job_employees: {:?}", &result);

        let reply = PageResult {
            data: result.data,
            next_cursor: Some(result.next_cursor),
        };

        Ok(reply)
    }

    /// Retrieves detailed profile information of an employee using their user ID.
    ///
    /// [查询用户详情](https://open.dingtalk.com/document/orgapp/query-user-details)
    ///
    /// This asynchronous function sends a POST request to the DingTalk API to fetch
    /// detailed profile information of an employee based on the provided `user_id`.
    /// The request includes an access token in the query parameters for authentication
    /// and specifies the response language.
    ///
    /// # Arguments
    ///
    /// * `user_id` - A string representing the unique identifier of the employee.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing a `UserGetProfileResponse` object if the request is successful,
    /// or an error if the request fails or if the response status is not successful.
    ///
    /// # Errors
    ///
    /// Returns an error if the response status is not successful, or if the request fails.
    pub async fn get_employee_userinfo(
        &self,
        user_id: String,
    ) -> Result<EmployeeUser, Box<dyn std::error::Error>> {
        let mut params: HashMap<&str, String> = HashMap::new();
        params.insert("language", "zh_CN".to_string());
        params.insert("userid", user_id);

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
                "Failed to response get employee count: {}",
                response.status()
            )
            .into());
        }

        #[derive(Serialize, Deserialize, Debug)]
        struct Response {
            errcode: i32,
            errmsg: String,
            result: EmployeeUser,
            request_id: Option<String>,
        }

        let result = match response.json::<Response>().await {
            Ok(res) => res.result,
            Err(e) => {
                error!("Failed to get user info: {}", e);
                return Err(e.into());
            }
        };

        Ok(result)
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct CountUserResponse {
    count: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct PageResult {
    #[serde(rename = "data_list")]
    pub data: Vec<String>,
    pub next_cursor: Option<i64>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EmployeeUser {
    #[serde(rename = "unionid")]
    pub union_id: String,
    #[serde(rename = "userid")]
    pub user_id: String,
    #[serde(rename = "name")]
    pub username: String,
    #[serde(rename = "avatar")]
    pub profile_url: String,
    pub state_code: String,

    #[serde(default)]
    pub manager_userid: Option<String>,

    pub mobile: String,
    pub hide_mobile: bool,
    pub telephone: String,

    #[serde(default)]
    pub job_number: String,

    #[serde(default)]
    pub title: String,

    #[serde(default)]
    pub email: Option<String>,
    pub work_place: String,
    pub remark: String,
    pub exclusive_account: bool,

    #[serde(default)]
    pub org_email: Option<String>,

    pub dept_id_list: Vec<i32>,
    pub dept_order_list: Vec<Department>,

    #[serde(default)]
    pub extension: Option<String>,

    #[serde(default)]
    pub hired_date: Option<u64>,

    pub active: bool,
    pub real_authed: bool,
    pub senior: bool,
    pub admin: bool,
    pub boss: bool,
    pub leader_in_dept: Option<Vec<LeaderInDepartment>>,

    #[serde(default)]
    pub role_list: Option<Vec<Role>>,
    #[serde(default)]
    pub union_emp_ext: HashMap<String, String>,
}
