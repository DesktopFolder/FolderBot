def af(slug: str):
    return open(f'auth/{slug}').read().strip()

def client_auth():
    #access_tok = open('auth/oauth.txt').read().strip()
    #access_id = open('auth/ttg-id.txt').read().strip()

    user_id = "FolderBot"
    # incorrect I guess
    # access_id = open('auth/idkid.txt').read().strip()

    owner_id = "desktopfolder"

    return {"client_id": af('nu_client_id'), "client_secret": af('nu_secret'),
            "bot_id": '263137120',
            "owner_id": '52534047'}
