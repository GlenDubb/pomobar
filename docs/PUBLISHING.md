# Publishing Pomobar to the Microsoft Store

A step-by-step checklist for getting Pomobar into the Microsoft Store as a
signed app, for someone who hasn't published a Windows app before.

**Cost:** free. **Your time:** about an hour, spread over a few sittings.
**Microsoft's review:** up to three business days.

## How signing works (why you don't need to buy a certificate)

Pomobar is packaged as **MSIX**. When you submit an MSIX package, Microsoft
signs it with its own certificate after it passes review, so people who install
Pomobar from the Store get a trusted, signed app with no security warnings, and
you never handle a certificate yourself. (A certificate only matters if you
later want to offer a direct download outside the Store; see the end.)

The packages the GitHub build produces are deliberately unsigned. That's
correct for Store submission.

---

## 1. Create your developer account (one time)

- [ ] Pick the personal Microsoft account (Outlook/Hotmail/Live) that will own
      your apps. Use one you'll keep long-term.
- [ ] Go to **storedeveloper.microsoft.com** and click **Get started for free**.
      Start here rather than in Partner Center: this is the free sign-up flow.
- [ ] Choose **Individual developer** and sign in.
- [ ] Verify your identity: you'll photograph a government-issued ID and take a
      selfie, on your phone, in good light, using the original document.
- [ ] Review the profile details it fills in for you.
- [ ] Click **Go to Partner Center dashboard**. You should land on *Apps and
      games*. If not, wait about five minutes and refresh.

Things to know before you finish:

- The **publisher display name** is shown as the developer on every app you
  publish. Individual accounts publish under your own name. **It can't be
  changed later, and neither can the account type.**
- If you later register *Williams Automation* as a business, you'd need a new
  **Company** account (proof of the business, such as a D-U-N-S number or
  registration documents, plus an email address on the business's own domain).
  Microsoft doesn't document a way to move an existing app between accounts, so
  plan on Pomobar staying where it's first published.
- Williams Automation still appears inside the app (About info, copyright and
  this GitHub project).

## 2. Reserve the name "Pomobar"

- [ ] In Partner Center: **Apps and games › New product › MSIX or PWA app**.
- [ ] Type `Pomobar`, click **Check availability**, then **Reserve product name**.
- [ ] The reservation lasts **three months**, so submit within that time.

If the name isn't available (someone may have reserved it without publishing),
reserve another one and let Claude or a developer know: the name also has to be
changed in `packaging/AppxManifest.xml`.

## 3. Put your Store identity into the package

Every Store app has an identity that the package must match exactly.

- [ ] In Partner Center, open your app, then **Product management › Product
      identity**. Copy these three values:
  - `Package/Identity/Name`
  - `Package/Identity/Publisher` (starts with `CN=`)
  - `Package/Properties/PublisherDisplayName`
- [ ] Open [`packaging/identity.json`](../packaging/identity.json) on GitHub,
      click the ✏️ pencil, paste the values in place of the placeholders, and
      **Commit changes**. (Or send the three values to Claude to do it.)

GitHub then rebuilds the package automatically with your identity (about 10
minutes).

## 4. Download the package

- [ ] On GitHub, open the **Actions** tab, then the newest **Build** run with a
      green tick.
- [ ] Under **Artifacts**, download **pomobar-store-package** and unzip it. Inside
      is `Pomobar_1.0.0.0.msixbundle`: the file you'll upload. It contains
      both the x64 and ARM64 versions.

To try the app first, download **pomobar-test-build** and run `pomobar.exe`.
Because that file isn't signed, Windows SmartScreen will warn you: click **More
info › Run anyway**. Store users won't see this, because Microsoft signs the
Store version.

## 5. Create the submission

In Partner Center, open Pomobar and click **Start submission**. Work through each
section. Ready-to-paste text for the listing is in
[`store/listing.md`](../store/listing.md).

**Pricing and availability**
- [ ] Markets: all. Visibility: public. Price: **Free**.

**Properties**
- [ ] Category: **Productivity**.
- [ ] Privacy policy URL: `https://github.com/GlenDubb/pomobar/blob/main/PRIVACY.md`
      (only strictly required for apps that collect personal data, but it does
      no harm to include it).
- [ ] Website: `https://github.com/GlenDubb/pomobar`
- [ ] Support contact: `https://github.com/GlenDubb/pomobar/issues`

**Age ratings**
- [ ] Answer the questionnaire. Pomobar has no violence, no user-to-user
      interaction, no purchases and doesn't share data, so it should come out as
      suitable for everyone.

**Packages**
- [ ] Upload `Pomobar_1.0.0.0.msixbundle` and wait for it to show as validated.

**Store listings** (English)
- [ ] Description and product features: copy from `store/listing.md`.
- [ ] Screenshots: **at least one** PNG, **1366 × 768 or larger** (up to 10;
      four is recommended). Easiest: run Pomobar, press <kbd>Win</kbd>+<kbd>Shift</kbd>+<kbd>S</kbd>,
      capture your full screen with a session running (so the taskbar badge
      shows), and save as PNG. One of the end-of-session pop-up is good too.
- [ ] Store logo (optional): `store/box-art-2160.png` works as the 1:1 box art.

**Submission options**
- [ ] Restricted capabilities: Pomobar declares `runFullTrust`. Explain:
      *"Pomobar is a native Win32 desktop application packaged as MSIX. The
      runFullTrust capability is required for desktop apps."*
- [ ] Notes for certification: copy from `store/listing.md`.

- [ ] Click **Submit for certification**.

## 6. After you submit

- Review can take up to three business days. Partner Center shows progress, and
  you'll get an email when Pomobar is live or if something needs fixing (the
  report says exactly what).
- Once published, Pomobar has a Store page and installs as a Microsoft-signed app.

## Releasing an update later

1. Change the code, then bump `version` in `Cargo.toml` (e.g. `1.0.0` → `1.0.1`).
   The Store only accepts a higher version than the last one.
2. Push to GitHub. Optionally tag it (`git tag v1.0.1 && git push --tags`) to
   get a draft GitHub release with the bundle attached.
3. Download the new bundle, start a **new submission** in Partner Center,
   replace the package, update *What's new*, and submit.

## Optional: signed downloads outside the Store

Not needed while you only publish through the Store. If you later want a signed
download on GitHub, the options for an individual in Australia are:

- An **OV code-signing certificate** from a certificate authority, roughly
  US$150–300 a year.
- **SignPath Foundation**, which signs qualifying open-source projects for free.
- Microsoft's **Azure Artifact Signing** (about US$10 a month) is currently only
  open to individual developers in the US and Canada.

## A note on the word "Pomodoro"

"Pomodoro" and "Pomodoro Technique" are trademarks of Francesco Cirillo, whose
company says that using "Pomodoro" in an app's name, subtitle or store
description requires a licence. That's why the app is called Pomobar and the
listing text in `store/listing.md` avoids the word. (This isn't legal advice.)
