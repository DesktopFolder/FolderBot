use rspotify::{prelude::*, scopes, AuthCodeSpotify, Config, Credentials, OAuth};

pub struct SpotifyChecker {
    pub spotify: AuthCodeSpotify,
}

impl SpotifyChecker {
    pub async fn new() -> Option<SpotifyChecker> {
        // spotify
        let rspotify_config = Config {
            token_cached: true,
            token_refreshing: true,
            ..Default::default()
        };
        let creds = Credentials::from_env().unwrap();
        let scopes = scopes!("user-read-currently-playing");
        let oauth = OAuth::from_env(scopes).unwrap();
        let spotify = AuthCodeSpotify::with_config(creds, oauth, rspotify_config);
        let url = spotify.get_authorize_url(false).unwrap();
        let res = spotify.prompt_for_token(&url).await;

        if let Ok(_) = res {
            Some(SpotifyChecker {
                spotify
            })
        } else {
            None
        }
    }
}
