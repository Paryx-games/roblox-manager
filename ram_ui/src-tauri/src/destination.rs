use ram_core::{api, assets_api, auth::RobloxClient};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchDestination {
    place_id: u64,
    name: String,
}

#[tauri::command]
pub async fn resolve_launch_destination(place_id: u64) -> Result<LaunchDestination, String> {
    if place_id == 0 || place_id > 9_007_199_254_740_991 {
        return Err("Enter a valid Roblox place ID".to_string());
    }
    let client = RobloxClient::new().map_err(|_| "Game lookup is unavailable".to_string())?;
    let universe_id = assets_api::resolve_place_universe(&client, "", place_id)
        .await
        .map_err(|_| {
            "The game could not be resolved. Check the place ID or retry later".to_string()
        })?;
    let name = api::resolve_universe_name(&client, universe_id)
        .await
        .map_err(|_| {
            "The game name could not be loaded. You can still use the place ID".to_string()
        })?;
    Ok(LaunchDestination { place_id, name })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_place_ids_are_rejected_before_network_access() {
        for id in [0, 9_007_199_254_740_992, u64::MAX] {
            assert!(resolve_launch_destination(id).await.is_err());
        }
    }
}
