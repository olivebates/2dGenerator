# Connect social accounts without Postman

Open **SoMe → Link accounts** in the updated Sepia app. Expand the service you want to connect. Connecting never posts anything. Only **Post** publishes approved drafts.

You need your own developer application for each service. Enter app secrets only into Sepia's masked fields, not into a chat. After a successful sign-in, Sepia remembers the secret in Windows Credential Manager; you can leave it blank when reconnecting with the same client ID.

## One shared website page for Tumblr and Threads

1. In one of these sections in Sepia, click **Save callback page for my website**. Or use `sepia-oauth.html` included in this project.
2. Upload that file to your own website. Keep the filename `sepia-oauth.html`.
3. Open it in your browser to confirm it works. Its address should look like `https://YOUR-WEBSITE/sepia-oauth.html`. Replace YOUR-WEBSITE with your real domain.
4. Use that exact address, including any trailing slash, wherever the steps below say **your callback address**.
5. Paste it into **HTTPS callback page on your own website** in Sepia for each of the two services.

The page needs ordinary static website hosting, not Python or a server application. Do not add analytics, external scripts or a login wall. Keep Sepia open on the same computer as the browser. The page passes a short-lived authorization result back to the app; Sepia exchanges it for credentials directly with the provider.

If your site uses a framework, make sure this file is served as a real HTML page instead of your site's fallback page. Uploading this page is a separate website change; Sepia only saves the file locally for you.

## Tumblr

1. Open [Tumblr applications](https://www.tumblr.com/oauth/apps).
2. Edit the application you already created.
3. Replace the old Postman address in **Default callback URL** with **your callback address**.
4. Put the same address in **OAuth2 redirect URLs**. Save the application.
5. Find **OAuth consumer key**. Copy it into Sepia's **OAuth consumer key** field under Tumblr.
6. Reveal the application's **secret key**. Copy it into Sepia's **Application client secret** field.
7. Click **Connect Tumblr** in Sepia.
8. Your browser opens Tumblr. Sign in and click **Allow**.
9. Return to Sepia. It should show that Tumblr is connected and fill in your blog identifier.
10. If a blog selector appears, choose the blog you want to post to. Click **Verify account**.

If registering a new app: use any clear application name, your real website, a description such as “Personal tool to publish game updates I approve,” and your own contact email. App Store URL, Play Store URL and Flurry Project ID can stay empty.

Tumblr's [OAuth documentation](https://github.com/tumblr/docs/blob/master/api.md#oauth2-authorization) describes its key/secret, redirect and refresh flow. Sepia requests basic account access, writing, and offline access.

## Threads

1. Open [Meta for Developers](https://developers.facebook.com/apps/). Select your app.
2. Add or open the **Access the Threads API** use case.
3. Open its settings and find the OAuth redirect URL setting. Add **your callback address** and save.
4. Under **App roles → Roles**, add your Threads account as a **Threads Tester** if it is not already allowed to use the app.
5. Sign in to [Threads](https://www.threads.com/). Open **Settings → Account → Website permissions** and accept the invitation if one is waiting.
6. In Meta's app dashboard, open **App settings → Basic**. Copy **Threads App ID** and **Threads App secret**. These are the Threads-specific values, not the ordinary Meta App ID/secret.
7. Enter them in Sepia's Threads section, along with **your callback address**.
8. Click **Connect Threads**. Sign in to Threads in your browser and approve access.
9. Return to Sepia. The user ID is filled in automatically. Click **Verify account**.

You do not need to find a Generate access token button. Sepia handles that exchange. The requested permissions are `threads_basic` and `threads_content_publish`. See [Meta's Threads authorization guide](https://developers.facebook.com/docs/threads/get-started/get-access-tokens-and-permissions/).

## X

X is manual. No developer app, token, API subscription or Postman is needed.

1. Find the X preview at the top of the list.
2. Review the caption. Save the image if wanted.
3. Click **Create new post**. Sign in to X if asked. The caption is filled in for you.
4. Attach the saved image yourself, review, and publish on X.
5. Return to Sepia and mark the approved preview as posted.

## Blogger

1. Open [Google Cloud Console](https://console.cloud.google.com/) and select the project for your Blogger app.
2. Open **APIs & Services → Library**. Search for **Blogger API**, open it, and click **Enable** if it is not enabled already.
3. Open **Google Auth Platform → Audience**.
4. Under **Test users**, click **Add users**. Add the exact Google email you use to manage your Blogger blog, then save. This fixes the screenshot's “Access blocked / developer-approved testers” error when the app is in Testing.
5. Open **Data Access → Add or remove scopes**. Add this scope and save:

   ```text
   https://www.googleapis.com/auth/blogger
   ```

6. Open **Clients → Create client**. Select **Desktop app**, give it a name, and create it. If your old client was a Web application for OAuth Playground, create a new Desktop client for Sepia.
7. Copy the Desktop client's **Client ID** and **Client secret** into Sepia's Blogger section. If Google offers a downloaded JSON file, those values are inside its `installed` object.
8. Click **Connect Blogger**.
9. In the browser, select the exact account you added as a test user. Approve access. A separate unverified-app warning may appear for your personal test app; only continue if it is the app you created.
10. Return to Sepia. It finds your blogs automatically. Choose the correct blog if more than one is listed, then click **Verify account**.

No OAuth Playground or Postman is needed. Google Desktop clients use the local browser callback; you do not register the website callback page for Blogger. While Google's app is in Testing, this publishing scope generally causes refresh tokens to expire after seven days. If Google revokes/expires the grant, click **Connect Blogger** again. See [Google desktop OAuth](https://developers.google.com/identity/protocols/oauth2/native-app) and [testing/personal-use guidance](https://support.google.com/cloud/answer/13464323).

## Instagram

Instagram is manual. No Meta app, professional account, callback or credential is needed.

1. Generate and review your Instagram draft in Sepia.
2. Click **Save image** to save the PNG.
3. Click **Copy post** to copy the caption.
4. Click **Open publishing page**, or open the Instagram phone app.
5. Create a post, upload the saved image, paste the caption and publish it yourself.
6. In Sepia, approve the draft and click **Mark as posted** to track completion.

The app's **Post** button never sends Instagram drafts.

## Shared image hosting for Threads and Blogger

Tumblr uploads directly and does not need this. Threads and Blogger can post text without hosting. To attach images automatically to Threads and Blogger, open **Link accounts > Shared image hosting**. Enter your Cloudinary cloud name and a restricted unsigned upload preset, then save settings. Existing hosting settings are retained. See [Cloudinary upload presets](https://cloudinary.com/documentation/upload_presets).

## If the browser does not return to Sepia

1. Keep Sepia open and wait for the sign-in result.
2. If your browser offers **Continue to Sepia**, click it.
3. If it still cannot connect, copy the full browser address after approving access.
4. Paste it into Sepia's masked **Full callback address from the browser** field and click **Finish connection**.
5. Do not share that address: it temporarily contains authorization information.

A callback page saying “Authorization received” means the browser handoff worked. Sepia still needs to exchange the code and check your account; its status is the final result. A provider error such as Google's “Access blocked” needs its settings corrected before retrying. Use **Cancel connection** to stop waiting, then click Connect for a fresh attempt. If port 43821 is busy, close the other Sepia sign-in attempt first.
