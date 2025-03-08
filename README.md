# Folderbot - Rust Twitch Bot

### Command Generation:
- !add / !edit -> Defaults to being prefixed with !. If another prefix is desired for a command, simply add the prefix. For no-prefix-support, use the prefix ^.

### Setup

`.env` should contain (for Spotify integration):

```
RSPOTIFY_CLIENT_ID=...
RSPOTIFY_CLIENT_SECRET=...
RSPOTIFY_REDIRECT_URI=http://localhost:8888/callback
```

`auth/id.txt` should contain: `channel-id` (just the raw string, no `#`, etc)

`auth/secret.txt` should contain: oauth secret (e.g. `oauth:abcdef0135003150530`)

`auth/user.txt` should contain: the bot username (e.g. `FolderBot`)

### Twitch Authentication

Uh, so the bot account is not the actual application...
