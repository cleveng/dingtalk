# Dingtalk SDK for Rust

## 用法

```rust
use async_dingtalk::DingTalk;

let dt = DingTalk::new("appid".to_string(), "app_secret".to_string());

// 获取授权链接
dt.get_redirect_url("https://example.com/callback".to_string(), Some("state".to_string()));

// 授权码获取用户信息
let userinfo = dt.get_contact_userinfo("me".to_string()).await.unwrap(); // me or union_id

// 免登录获取用户信息
let userinfo = dt.set_corp_id("corp_id".to_string()).get_userinfo("code".to_string()).await.unwrap;
```
