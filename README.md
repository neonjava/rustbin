# RustBin

**Fast, simple paste sharing. Built with Rust.**

RustBin is an account-free paste service with a Rust/Axum API, a small static frontend, and Firestore. The no-billing deployment uses Vercel Hobby Rust Functions with a Firebase Spark Firestore database. Firebase Hosting and Cloud Run remain an optional billing-enabled path. It stores text; it never runs submitted code. No trackers, ads, or analytics are included. Hosting infrastructure still observes ordinary request metadata.

> Development status: v0.1.0 source. GitHub CI verifies the Rust build and tests; a live Firestore-backed deployment remains to be verified.

## Features

- Secure random 8-character public IDs, with atomic Firestore collision handling
- Text and code pastes up to 512 KiB by default
- 16 allowlisted languages; 10-minute to 30-day expiration or never
- Shareable clean URLs, raw text URLs, copy controls, and delete tokens
- Responsive light, dark, and system themes
- Server-side validation, read-time expiration, request size and basic rate limits, security headers

## Architecture

```text
Vercel Hobby
       ├── static HTML/CSS/JS
       └── Rust + Axum Function (/api/* and /raw/*)
                    └── Firebase Spark Firestore
                 pastes/{id}
```

Firestore documents use the public ID as their document ID. Fields: `title`, `content`, `language`, `created_at`, `expires_at`, `delete_token_hash`, and `views`. The raw delete token is returned once and never stored. `views` is reserved for future use and currently remains zero.

## Quick start

Prerequisites: current stable Rust, a Firebase/GCP project with Firestore Native mode, and `gcloud`.

```bash
git clone https://github.com/neonjava/rustbin.git
cd rustbin
cp .env.example .env
# Set FIREBASE_PROJECT_ID in .env
gcloud auth application-default login
cargo run
```

Open `http://127.0.0.1:8080`. The Rust process serves `public/` locally. ADC must be an authorized-user credential locally; Cloud Run uses its attached service identity. Do not place service-account keys in this repository.

For an isolated local database, run the Firestore emulator and set `FIRESTORE_EMULATOR_HOST=127.0.0.1:8085` in `.env`. Start the emulator with `firebase emulators:start --only firestore` after installing the Firebase CLI and configuring the project. Tests use an in-memory store and never contact Firestore.

## Configuration

| Variable | Default | Purpose |
| --- | --- | --- |
| `FIREBASE_PROJECT_ID` or `GOOGLE_CLOUD_PROJECT` | required | Firestore project |
| `RUSTBIN_HOST` | `0.0.0.0` | Listen address; `.env.example` uses loopback locally |
| `PORT` | `8080` | Cloud Run listen port |
| `RUSTBIN_MAX_PASTE_SIZE` | `524288` | Maximum UTF-8 content bytes (up to 900000) |
| `RUSTBIN_ID_LENGTH` | `8` | Random ID length, 8–32 |
| `RUSTBIN_RATE_LIMIT` | `30` | Creations per peer IP per 10 minutes, per instance |
| `FIRESTORE_EMULATOR_HOST` | unset | Emulator host:port |
| `RUST_LOG` | `info` | Tracing filter |

The in-memory rate limit is per Rust process and uses the immediate peer address. On Vercel Functions it is only a light safeguard because instances scale independently; use a managed edge rate limit before accepting significant public traffic.

## API

`GET /health` returns `{"status":"ok"}`.

`POST /api/v1/pastes` accepts JSON:

```json
{"title":"Hello Rust","content":"fn main() {}","language":"rust","expiration":"1d"}
```

Language values: `text`, `rust`, `java`, `c`, `cpp`, `python`, `javascript`, `typescript`, `html`, `css`, `json`, `yaml`, `toml`, `bash`, `sql`, `markdown`. Expiration values: `10m`, `1h`, `1d`, `7d`, `30d`, `never`. The response includes a generated `id`, relative `url`, `raw_url`, `expires_at`, and a one-time `delete_token`.

`GET /api/v1/pastes/{id}` returns public paste fields. `GET /raw/{id}` returns only `text/plain`. `DELETE /api/v1/pastes/{id}` requires `Authorization: Bearer <delete_token>` and returns 204 on success. Errors use `{"error":{"code":"...","message":"..."}}`.

## Free deployment: Vercel + Firebase Spark

This is the recommended path when you do not want to link a Cloud Billing account or use a paid hosting plan. Vercel serves the static frontend and runs the Rust/Axum API as an official Rust Function. The public URL is a `*.vercel.app` address; paste and raw routes stay on that origin. Vercel's Rust runtime is in beta, and Hobby limits apply. Firebase Hosting at `*.web.app` is **not** the public app on this path: its Cloud Run rewrite requires billing.

1. In Firebase project `rustbin-fa262`, create a **Firestore Standard `(default)` database** in production mode. The server uses IAM, so client security rules can deny direct reads and writes. Deploy `firestore.rules` with `firebase deploy --only firestore:rules --project rustbin-fa262` if you use the Firebase CLI.
2. In Google Cloud IAM, create a dedicated service account with only `roles/datastore.user` on this project. Generate a JSON key for that account. Treat it as a secret; never add it to Git or paste it into a public issue.
3. Import `neonjava/rustbin` into a personal Vercel Hobby project. The checked-in `vercel.json` configures the Rust Function and static routes. Set the Vercel environment variable `FIREBASE_PROJECT_ID=rustbin-fa262`. Set `FIRESTORE_SERVICE_ACCOUNT_JSON` to the **entire JSON key** using Vercel's protected environment-variable UI; do not put it in source files or deployment commands. Apply both to Production (and Preview only if you intentionally use the same database).
4. Deploy, check `/health`, then create a paste, open it, open `/raw/{id}`, and delete it using the returned token. Keep the production URL from Vercel's dashboard.

The server signs a short-lived OAuth assertion using the protected credential and caches the resulting access token. The key is never returned by the API or stored in Firestore. Rotate the key through Google Cloud IAM and replace the Vercel variable if it is exposed. [Vercel Rust runtime](https://vercel.com/docs/functions/runtimes/rust) and [Google's service-account OAuth flow](https://developers.google.com/identity/protocols/oauth2/service-account) describe the underlying mechanisms.

## Optional Cloud Run deployment (billing required)

1. Create a Firebase project and enable Firestore Native mode and Hosting. Cloud Run requires a linked Cloud Billing account, which upgrades a Spark project to Blaze; review billing before enabling it. Then enable Cloud Run, Cloud Build, and Artifact Registry. Create an Artifact Registry Docker repository named `rustbin`.
2. Create a runtime service account with `roles/datastore.user`. Set up GitHub Actions Workload Identity Federation for `neonjava/rustbin` and a deploy service account with permissions for Cloud Build, Artifact Registry, Cloud Run deployment, Firebase Hosting deployment, and `iam.serviceAccounts.actAs` on the runtime account. Scope these grants to the project and resources as narrowly as practical.
3. Set GitHub environment `production` variables: `GCP_PROJECT_ID`, `GCP_REGION` (`us-central1` to match `firebase.json`), `GCP_WORKLOAD_IDENTITY_PROVIDER`, `GCP_DEPLOY_SERVICE_ACCOUNT`, `GCP_RUNTIME_SERVICE_ACCOUNT`.
4. Run the `Deploy` workflow or push to `main` after CI succeeds. The workflow builds a container, deploys `rustbin-api`, and deploys Hosting. Change both `firebase.json` rewrite regions if deploying Cloud Run elsewhere. Cloud Run is publicly invokable so Hosting can reach it; use ingress and edge controls appropriate to your deployment.
5. Visit `https://rustbin-fa262.web.app/` and verify create, open, raw, expiration, and deletion with actual test pastes.

The Docker image is a multi-stage Rust build with a non-root runtime for the optional Cloud Run path. Locally: `docker build -t rustbin .` then run with `FIREBASE_PROJECT_ID` and an accessible credential or emulator. Hosting serves `public/`; `/api/**` and `/raw/**` rewrite to Cloud Run; other paths serve `index.html`.

The `firebase.json` Cloud Run rewrites are for this optional path only. Firebase Hosting/Cloud Run rewrite syntax follows [Firebase's current Hosting configuration](https://firebase.google.com/docs/hosting/full-config). Firestore document operations use the [Firestore REST API](https://cloud.google.com/firestore/docs/reference/rest/v1/projects.databases.documents).

Firestore TTL is not configured automatically. Read-time expiration blocks access; optional Firestore TTL cleanup can be configured separately for `expires_at` after checking compatibility with non-expiring documents.

## Testing and security

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo build --release
```

The frontend inserts paste content with `textContent`, and raw responses are `text/plain` with `nosniff`. The server validates IDs, language, title, size, and expiration. Delete tokens are random, hashed before storage, and verified with constant-time comparison. Raw tokens and paste content are not logged. A strict CSP is used on both Hosting and Axum responses.

A local screenshot will be added after a browser-based visual pass is available; no placeholder image is published.

## Contributing and roadmap

See [CONTRIBUTING.md](CONTRIBUTING.md), [SECURITY.md](SECURITY.md), and [CHANGELOG.md](CHANGELOG.md). Planned possibilities include a CLI (`cat main.rs | rustbin`), burn-after-reading, encrypted pastes, private modes, accounts, API tokens, moderation, and SDKs. These are outside the MVP.

MIT License © 2026 neonjava.
