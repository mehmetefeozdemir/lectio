# Store questionnaire answers, with the reasons

Answers for the forms in App Store Connect and Google Play Console for Lectio 1.0.0 (ADR-040). Each answer says why,
from how the app actually works, so it can be checked again when the app changes.

**The facts the answers rest on**
- No account, no sign-in, no server of Lectio's own. Nothing is sent to the developer.
- Recordings, notes, slides, photos, transcripts, quizzes and flashcards are stored only in the app's private storage.
- The only network use in the phone apps: downloading the speech and study models from `huggingface.co` over HTTPS
  when the user asks for a transcript or a study tool. No user data is sent with those requests.
- No ads, no analytics, no crash reporting, no third-party SDKs that collect data (the Rust core, JNA and
  Jetpack/SwiftUI only).
- The microphone is used only while the user records; the camera and photo picker only when the user adds a photo.

## App Store Connect

### App Privacy ("nutrition label")
**Data Not Collected.** (`ios/fastlane/app_privacy_details.json`)
- Apple defines "collect" as transmitting data off the device in a way that the developer or a third party can access
  for longer than needed to serve the request. Lectio transmits no user data at all; audio, notes and photos never
  leave the device unless the user shares an export themselves.
- The model download sends only a normal HTTPS request (no identifiers, no user content), so it is not collection.
- Tracking: **No.** No data is linked with other companies' data, no advertising, no data brokers.

### Age rating
All content descriptions **None**, and every yes/no question **No** → **4+**. (`ios/fastlane/app_rating_config.json`)
- No violence, sexual content, profanity, horror, drugs, gambling, contests, medical information, weapons or mature
  themes: Lectio shows only what the user writes and records.
- Unrestricted web access: **No** (no browser; the only network use is the model download).
- User-generated content: **No**. The user's notes are private on their device; nothing is shared with or shown to
  other users.
- Messaging/chat: No. Advertising: No. Parental controls: No. Age assurance: No. Loot boxes: No.
- Health or wellness topics: No.

### Export compliance
`ITSAppUsesNonExemptEncryption = NO` is in the app's Info.plist: the app uses only HTTPS (the system's TLS) for the
model download and no encryption of its own. So App Store Connect doesn't ask on each upload.

### Category
Primary **Education**, secondary **Productivity**.

### Content rights
"Does your app contain, show, or access third-party content?" **No**: the app shows only the user's own content. The
downloaded models are program files (open model weights under their licences), not content shown to users.

## Google Play Console

### Data safety
- "Does your app collect or share any of the required user data types?" **No.**
  - Collect = transmitted off the device; share = transferred to a third party. Lectio does neither: everything stays
    in the app's private storage. Data processed only on the device is not "collected" in Play's definition.
- "Is all of the user data collected by your app encrypted in transit?" Not asked when nothing is collected (if the
  form asks, **Yes**: the only network traffic, the model download, is HTTPS).
- "Do you provide a way for users to request that their data is deleted?" Nothing is held by the developer; users
  delete data in the app or by uninstalling (the privacy policy says how).
- Privacy policy URL: `https://mehmetefeozdemir.github.io/lectio/privacy.html`.

### App content
- **Ads:** No ads.
- **App access:** All functionality is available without special access (no login).
- **Content rating (IARC questionnaire):** category **Reference, News, or Educational**; answer **No** to violence,
  sexuality, language, controlled substances, gambling, user interaction/sharing, location sharing, digital purchases.
  Expected result: Everyone / PEGI 3 / USK 0.
- **Target audience:** **13–15, 16–17 and 18+** (students). Not under 13: choosing children's ages brings the Families
  policy requirements, and the app isn't designed for young children (it records audio and downloads large models).
- **News app:** No. **Health app:** No. **Financial features:** None. **Government app:** No.
- **Data safety:** as above.

### Foreground service declaration (Android 14+)
Type: **microphone** (`FOREGROUND_SERVICE_MICROPHONE`). Text for the declaration form:

> Lectio records lectures for students. While a recording is running, a foreground service of type "microphone" keeps
> the recording going when the screen turns off or the student switches apps, so the whole lecture is captured. The
> service starts only when the user taps Record and stops when they tap Stop (in the app or in the ongoing
> notification). It shows a notification for the whole time. It is not used for anything else.

Play also asks for a **short video** showing the feature: record the screen while you tap Record, lock the screen
for a few seconds, unlock, and show that the recording time kept counting and the notification with Stop is there.
Upload it (unlisted on YouTube, or as Play asks) and paste the link.

### Permissions in the manifest, and why
| permission | why |
|---|---|
| RECORD_AUDIO | recording the lecture; asked the first time the user taps Record, after an explanation |
| FOREGROUND_SERVICE, FOREGROUND_SERVICE_MICROPHONE | keep recording with the screen off (above) |
| POST_NOTIFICATIONS | the ongoing "Recording a lecture" notification with Stop (Android 13+) |
| WAKE_LOCK | keep the CPU awake while recording with the screen off |
| INTERNET | download the speech and study models (HTTPS, checked by SHA-256) |

No camera permission: the photo is taken by the system camera app (camera intent), and the Photo Picker needs no
permission.

### Store listing
Category **Education**; tags: education, productivity, note-taking. Contact email: `mehmetefecan@gmail.com`. Website:
`https://mehmetefeozdemir.github.io/lectio/`.
