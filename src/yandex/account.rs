use super::*;

impl Yandex {
    /// Проверка токена и профиль. Возвращает uid для библиотечных вызовов.
    pub async fn account_status(&self) -> YResult<(ProfileDto, i64)> {
        let result = self.get_result("/account/status").await?;
        let status: YAccountStatus =
            serde_json::from_value(result).map_err(|e| ApiError::Msg(e.to_string()))?;
        let uid = status.account.uid.unwrap_or_default();
        let avatar_url = status.account.avatar_url.or_else(|| {
            status
                .account
                .avatar_id
                .map(|id| format!("https://avatars.yandex.net/get-yapic/{id}/islands-200"))
        });
        let avatar_url = avatar_url.map(|url| {
            let normalized = if url.starts_with("//") {
                format!("https:{url}")
            } else if !url.starts_with("http") {
                format!("https://{url}")
            } else {
                url
            };
            normalized.replace("%%", "200x200")
        });
        let display_name = status
            .account
            .display_name
            .or(status.account.full_name)
            .or(status.account.login.clone())
            .unwrap_or_default();
        Ok((
            ProfileDto {
                uid,
                login: status.account.login.clone().unwrap_or_default(),
                display_name,
                avatar_url,
                has_plus: status.plus.and_then(|p| p.has_plus).unwrap_or(false),
            },
            uid,
        ))
    }
}
