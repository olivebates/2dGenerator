# Sepia Chat

A standalone Windows desktop app written entirely in Rust, with **Code** and **SoMe** tabs at the top right. It runs the installed **Codex app-server as a local child process**, communicating over stdin/stdout. There is no Python, OpenAI API key, or direct OpenAI API integration. Codex uses your ChatGPT login and still requires internet access and available Codex usage on your account. Automatic social publishing uses the social platforms' own APIs and user credentials.

## Run

With Rust, the Visual Studio C++ build tools, and Windows SDK installed, open **Developer PowerShell for VS 2022** in this folder:

```powershell
cargo run --release
```

The compiled app is `target/release/sepia-chat.exe`. It can be launched directly. Codex must remain installed on the machine.

If Cargo is not on PATH, use `& "$env:USERPROFILE/.cargo/bin/cargo.exe" run --release`.

The app detects Codex bundled with the VS Code OpenAI extension, then falls back to `codex.exe` on PATH. To specify a different installation, set `SEPIA_CODEX_PATH` to the full executable path before launching. No VS Code window needs to be open.

## Use

1. Select a project with **Choose project folder** at the top left.
2. Use **Sign in with ChatGPT** if needed, then open the sign-in link. Existing ChatGPT Codex credentials can be reused. API-key authentication is deliberately disabled for this child process.
3. Type your message and choose a send button. **New Chat** and **No code** are on the left beneath the input; **Ask me questions**, **Enhance**, and **Code** are on the right. Code uses a stronger sepia fill. The sidebar branding text has been removed.

| Button | Exact appended text | Project access |
| --- | --- | --- |
| Ask me questions | Ask me questions about it if there are any. Dont code anything yet. | Read only, no sandbox escalation |
| Enhance | How can I make this more satisfying? Don't code anything yet. | Read only, no sandbox escalation |
| Code | Go ahead and code it. | Workspace writes, user approvals when Codex requires them |
| No code | Dont code anything yet. | Read only, no sandbox escalation |

All four send buttons continue the same Codex thread. **New Chat** keeps the folder. Choosing a different folder creates a new conversation; existing conversations retain their own folders. Selecting an older conversation restores its folder and resumes its Codex thread on the next message.

While a prompt runs, **No code** becomes **Steer**. Type guidance and click Steer to deliver it to the active turn at its next opportunity, without appending a mode instruction or changing permissions. Multiple steering messages are delivered in order; pending or unsuccessful deliveries are labeled in the transcript. Enter only inserts a new line. The other send buttons remain disabled, while **Stop** remains available. Stop preserves partial output and edits and adds a **Stopped** marker.

Images opened by Codex and image content returned by tools appear inline in the conversation. Click a preview to enlarge it. Local viewed images are snapshotted into saved chat history so temporary screenshots and crops remain available after their source files disappear. Missing or unsupported images show an unavailable message.

Answers stream into the transcript. **Stop** interrupts the current turn; it does not undo changes already made. Approval dialogs show the requested action, with Allow once and Deny. Unsupported server requests are rejected rather than silently approved. Standard workspace edits can run under the Code sandbox without individual prompts; elevated actions and commands requiring approval appear in the app.

Chat text and drafts are saved locally under `%LOCALAPPDATA%/Sepia/Sepia Chat/data/chats.json`. Codex maintains its corresponding thread history in its own storage. Keep both to resume conversations. Saves replace the chat file atomically; an unreadable chat file is preserved and sending is disabled until it is repaired.

Replies render Markdown with headings, lists, emphasis, tables, and formatted code blocks. Each code block has its own copy control; Copy message copies the original complete message. The transcript stays centered at a maximum width of 850 logical pixels, with padded message cards. The input remains four rows with its existing behavior.

The sidebar uses a soft highlight for the selected chat and shows the project name underneath each title. Long titles and folders are shortened visually; hover to see their full text. Buttons have subtle hover transitions. While Codex works, a small spinner and a distinct Stop button appear above the input.

## Development

```powershell
cargo test
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

The ignored connection smoke test starts the installed Codex process and checks its initialization protocol without sending a model prompt:

```powershell
cargo test local_codex_handshake -- --ignored
```

The app uses the Codex app-server protocol generated by the locally installed CLI. It does not attach to an existing VS Code chat. Read-only filesystem enforcement relies on Codex's OS sandbox; if the sandbox cannot start, the app reports the Codex error rather than switching to unrestricted execution.

## Verification status on this machine

- Automated tests cover chat behavior, UI rendering, revisions and reapproval, platform validation, forbidden Reddit drafts, emoji handling, disclosure, cache retention, credential storage, sanitized failures, and native image-output validation and PNG export.
- Formatting and Clippy checks passed, and the release executable was built successfully.
- The app window was launched and inspected at normal, small, and wide sizes with the updated action row, spacing, and sidebar.
- A live Codex/ChatGPT test generated eleven sample platform drafts and revised one without changing the others. Structured generation, both required links, character/emoji checks, reapproval and shared image handling passed. No social post was sent. Real publishing requires your accounts and publishing credentials, so the external publishers have not been tested against live accounts.

## SoMe workflow

1. Sign in to Codex with ChatGPT on the Code tab if needed. SoMe uses a separate read-only Codex conversation, never your coding thread. Generate text and Update can inspect relevant files in the project selected at the top left using read-only local tools. Writes, network access and permission escalation remain disabled; the drafting worker cannot publish. Switching projects starts a fresh drafting context. Prompt improvement and image generation retain their separate behavior.
2. Open SoMe and enter real game-update information and a full `https://` website URL for the batch. Typing saves locally and sends no generation requests.
3. Click **Generate text**. This creates one tailored draft for each of Medium, Substack, Substack Notes, Facebook, Tumblr, Mastodon, Threads, Bluesky, X, Blogger, and Instagram, plus a separate draft for every subreddit saved in settings. Missing/stale subreddit rules are fetched first. Existing previews remain intact if generation fails.
4. Review the previews. Each has **Approve**, **Copy post**, and a revision field. **Approve all** approves eligible drafts without warnings; warnings require individual review. Drafts with missing links, excessive length, missing images/disclosures, unknown subreddit rules, or explicit bans cannot be approved.
5. Write revision requests locally, then click **Update** at the bottom to send all requested revisions together. Only affected previews change, and each requires approval again. Generate text applies composer changes; approved previews remain independently postable. Changing account/community settings revokes approval and prepares a rule-recheck revision for existing unposted drafts.
6. Click **Post approved** to publish only approved automatic destinations. This does not send a new message to Codex. Each result is recorded independently. Manual destinations provide Copy post and Open publishing page; after posting yourself, use Mark as posted.

The writing brief calls for simple, natural sentences based only on supplied facts, followed by an editing pass to remove generic filler. Long devlogs are drafted for Medium, Substack (website only), and Blogger. Short platforms receive concise posts. One emoji is retained where allowed; destinations that disallow emoji receive none. Eligible posts include your exact website link and end with a simple invitation to `https://discord.gg/dCmMS2Rhep`. Medium gets an explicit AI-assistance disclosure and is intended for free publishing, not the Partner Program paywall.

**Generate text** and **Generate image** are independent buttons. Generate text creates/replaces text drafts without generating an image, retaining an existing shared image if present. Generate image uses native Codex with your existing ChatGPT login to create the actual picture described in Image prompt, then adds it to compatible unposted drafts without changing their text or pending revisions. It also works before generating text. **Remove image** detaches it from unposted drafts, leaving the files and published history intact. Adding, replacing or removing an attachment clears affected approvals. No OpenAI API key is used. Failed image generation keeps the current picture and posts.

The same generated picture is reused on image-capable destinations. Automatic publishers attach it where allowed; manual drafts offer **Save image**. Reddit remains text-only unless the supplied rules explicitly allow an image. Changing a shared image revokes approvals on affected unposted drafts. Update revises captions while keeping the existing picture; change Image prompt and click Generate image for a new picture. Existing saved text cards remain available until you generate a replacement.

Image uploads need `write:media` on Mastodon in addition to the text-posting scopes. Threads and Blogger use the optional Cloudinary settings under Shared image hosting. Without hosting they show text-only previews. Facebook, Tumblr, Mastodon and Bluesky upload images directly. X is manual; save its image and attach it in the browser. No image is uploaded until Post is pressed.

If Facebook's token permission picker does not offer publishing scopes, first check the selected Meta app under **Use cases → Manage everything on your Page → Customize → Permissions and features**. Add `pages_manage_posts` and `pages_read_engagement`, return to Graph API Explorer, select that same app, and generate a fresh token. `business_management` and `pages_show_list` alone do not authorize publishing. If the Page use case is unavailable because the existing app has an incompatible use case, create a separate Page-management app. See [Meta's use-case guide](https://developers.facebook.com/documentation/development/create-an-app/).

## Account connections

The SoMe composer has separate post and image prompts. **Improve prompt**, to the left of Generate text, rewrites both fields using the local ChatGPT/Codex login. It does not generate posts or images. Both prompts remain editable. The image brief describes the scene, composition and style; no lettering is added unless requested. Edit the image prompt and click Generate image to replace only the picture. Editing that brief does not prevent publishing the currently previewed posts.

**Link accounts** is above the previews and opens account settings. **Back to generation** returns to the drafts. Each platform has setup instructions, policy links, and additional account/community-rule notes.

**Tumblr, Threads and Blogger have Connect buttons. Postman is not required.** Register your own developer app once, enter its client ID and secret where required, then approve access in your normal browser. Sepia discovers account/blog IDs, offers a blog selector when several are returned, and stores access tokens, refresh tokens and app secrets in **Windows Credential Manager**. No credentials are sent to Codex or saved in `social.json`. Verify/Post refresh browser-issued tokens when needed; expired/revoked grants may require reconnecting. Manual token entry remains available, and replaces the browser grant. Disconnect deletes both the access token and browser grant locally; revoke access on the provider website if desired.

Follow [the simple setup guide](SOCIAL_SETUP.md) for the social services. Blogger returns to `http://127.0.0.1:43821/oauth/callback`. Tumblr and Threads use the included [sepia-oauth.html](sepia-oauth.html), hosted on **your own HTTPS website** and registered as the redirect URL. The page forwards the result to Sepia's temporary loopback listener. Keep Sepia running during sign-in. If the browser blocks the handoff, paste the final callback address into **Finish connection**. Callback origin/path and a fresh random state are checked; Blogger also uses PKCE S256. Sign-in expires after five minutes and can be cancelled. The callback page does not receive app secrets or access/refresh tokens; its one-time code and state should not be shared, logged by analytics, or loaded through external scripts.

The remaining automatic connectors accept platform tokens/app passwords as before. Verify account is a read-only check (or Bluesky sign-in), not a post, and does not guarantee publishing permission. External browser sign-ins and publishing have not been tested against your live accounts; automated tests cover callback handling, PKCE, account discovery and token rotation.

| Platform | Mode and required setup |
| --- | --- |
| Facebook | Automatic to a **Page**: Page ID and Page access token with publishing permission. No personal-profile publishing. |
| Threads | Automatic: Threads user ID and user token with `threads_basic`, `threads_content_publish`. Own accounts may require app tester setup; broader access may require app review. |
| Bluesky | Automatic: handle and app password. Connector signs in through `bsky.social`, then uses the account's returned PDS. |
| Mastodon | Automatic: server origin and a personal-application token with `read:accounts`, `write:statuses`, `write:media`. Supply your server's additional rules. |
| Tumblr | Automatic: blog identifier and OAuth 2.0 user access token with write permission. |
| X | Manual: Create new post opens the browser composer with the caption filled in. Attach the saved image yourself. No API credentials or credits needed. |
| Blogger | Automatic: blog ID and Google OAuth user token with Blogger scope; enable Blogger API in your Google project. |
| Instagram | Manual: save the generated image, copy the caption, upload both to Instagram and mark the draft as posted. No Meta app, professional account or credential is needed in Sepia. |
| Medium | Manual copy: no new API integrations/tokens. |
| Substack | Manual copy: website only. In Substack, turn **off** sending via email/app inbox before publishing. |
| Substack Notes | Manual copy. |
| Reddit | Manual copy, separately tailored to each saved subreddit. No Reddit posting credential or automatic posting endpoint. |

## Subreddit cache and policy checks

Save subreddit names on Link accounts. Public rules are cached for **seven days** and refreshed on Generate when missing/stale, or explicitly with Refresh cached rules. A blocked/failed Reddit fetch preserves the old cache and asks you to paste current rules, including sidebar/pinned-thread restrictions. Successfully fetched rules are retained even if Codex generation later fails.

Explicit promotion/link bans, or a rule-aware generated determination that promotion/links/AI content are forbidden, replace the draft with exactly:

> This subreddit doesnt accept promotions

Those placeholders have no promotional links and cannot be approved. Generated interpretations of rules can be wrong; inspect conditional rules, megathreads, flair requirements, and AI restrictions yourself. The app provides policy-aware drafting, source links and structural validation, **not a guarantee of compliance or immunity from moderation**. Provider policies can change; use the linked policies and additional-rule fields to keep the brief current.

## Storage and publishing reliability

SoMe inputs, drafts, revisions, approvals, cached rules and account identifiers live beside `chats.json` in `social.json`; generated images live in `social-images`. Credentials live separately in Windows Credential Manager. Unreadable saved data is preserved and writing/generation is disabled rather than overwriting it.

Publishing records are saved before sending, and completed/uncertain posts are not silently resent. After a timeout or an interrupted app session, check the destination website. Only use **I checked the platform: not posted. Reset for retry** after confirming the post is absent. A successful publish cannot be undone by this app. Keep the app running while generation or publishing is in progress.

The optional live drafting integration test consumes Codex usage but never posts. It generates a sample batch and revision and writes its inspection artifact under ignored `.tools/`:

```powershell
cargo test local_codex_batch_and_revision -- --ignored
```

## Style references and image history

**Upload style reference** chooses a PNG, JPEG or WebP image (up to 32 MB and 8192 pixels per dimension), validates it and copies it into local app storage. The selected reference is previewed and remembered. **Remove style reference** stops using it for future images. Only Generate image sends it to Codex; it guides palette, rendering technique, texture and lighting while the image prompt defines the new scene. The reference is not an edit target. Sepia checks the native tool's observed input-image paths to confirm the selected reference was actually used; otherwise it preserves the current image and reports an error.

The **Last 5 generated images** gallery is saved locally. Click a thumbnail or use Previous image / Next image to select a picture without rewriting text. New images appear first; selecting an older picture does not change creation order. Existing generated PNGs are recovered on first load where available. Only the last five are listed; older files remain because saved/published records may reference them.

Each unposted preview has **Remove image from this post**. That post stays text-only when selecting or generating another image, restarting the app, or regenerating text. **Add image to this post** restores the current shared picture where the destination rules permit it. Attachment changes clear that post's approval. Published records are preserved.
## Prompt improvement controls

**Improve text**, **Improve image** and **Improve both** let you choose exactly which input fields to rewrite. Unselected fields stay unchanged, including when only the image brief is filled. Only the outgoing image-generation request gets this exact suffix, added once. The editable field and improved prompt stay free of the preset:

```text
Kodak Portra 400 85mm f/1.4 dutch angle 60% negative space watercolor. Hand-painted watercolor and colored-pencil illustration.. Translucent washes, visible pigment granulation, softly bleeding edges, loose pencil outlines and slightly uneven brushwork. Simplified shapes, restrained detail and a limited earthy palette. Leave generous unpainted space. No glossy 3D rendering, photographic depth of field. No highlights just flat cell shading and no paper grain texture.
```

Style-reference selection and removal are saved immediately. Sepia keeps its own validated local copy and restores the reference when reopened, even if the original selected file is moved.

## Independent posting and image fallback

Post approved sends the reviewed previews, using each draft's saved batch links; editing composer inputs does not cancel the send. Each automatic destination is checked separately. Missing credentials, invalid drafts or failed requests leave a red error on that preview while the other ready posts continue. The result summary appears beside the Post controls as well as at the top. Manual destinations remain copy/upload only.

Image attachments follow account capability and community restrictions. Tumblr uses its native multipart image upload, without external hosting. Threads and Blogger use a public image URL; if Shared image hosting is not configured, their previews omit the image and explain why. Missing local image files are omitted during send preparation. Failures in separate image-upload stages (Cloudinary, Mastodon, Bluesky) can fall back to a single text-only publishing request; the saved result reflects that no image was posted. Final publish requests are never automatically retried after an uncertain result, including Facebook photo uploads and Tumblr multipart posts, because they might already have created the post. Verify those sites before resetting for retry.

Automated checks include mixed ready/failed batches, red-error persistence, image-hosting capability and Tumblr multipart request structure. No live social post was sent during verification.
X appears first in previews, followed by Instagram. Manual previews offer **Create new post**, **Copy post**, and **Save image**. Set an optional HTTPS editor URL in Link accounts for publication-specific editors. Sites without a stable public composer URL open their publishing page; choose Create there. Automatic previews offer **Send this post** after individual approval; this sends only that draft and preserves other approvals. **Post approved** still sends the approved automatic batch.

The Code tab provides Model and Reasoning selectors populated from the installed Codex model catalog. Supported reasoning levels follow the selected model, and selections persist across restarts. Code actions use **Approve for me** (Codex automatic risk-based approval review with on-request approvals); Ask, Enhance, and No code remain read-only. Automatic review can still deny a request or require user input.
