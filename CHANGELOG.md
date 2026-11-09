# Changelog

All notable changes to FurFiles are recorded here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.1] - 2026-11-09

### Changed

- Backend dependencies moved to their current major versions: axum 0.8,
  sqlx 0.9, tower-http 0.7, reqwest 0.13, rand 0.10, sha2 0.11,
  chacha20poly1305 0.11 and base64 0.23. Existing databases, stored AI keys
  and the generated secret key file keep working unchanged. HTTPS now trusts
  the operating system's certificate store, so a custom image must keep
  `ca-certificates` installed.
- Frontend toolchain on current versions: pnpm 12, vitest 5, jsdom 31,
  vue-tsc 3 and TypeScript 6, plus Vue 3.5.45, Vue Router 5.4 and Vite 8.3.6.
  TypeScript stays on 6.x because vue-tsc does not run on the native
  TypeScript 7 yet. pnpm 12 holds back releases younger than its default
  minimum release age, so a version published in the last day waits for the
  next update.

### Fixed

- Reading a facility's website stops at the 2 MB page limit instead of
  downloading the whole response first, so a site that sends an enormous or
  endless page can no longer exhaust the server's memory.

## [0.2.0] - 2026-11-09

### Added

- An MCP (Model Context Protocol) server at `/mcp`, so an AI assistant such
  as Claude can read the household's pets, records, weights, reminders,
  ongoing care, contacts, insurance and costs, and add or correct records,
  weigh-ins, reminders and care entries. Off until `FURFILES_MCP_TOKEN` is
  set; every request needs that token as a bearer token, and browser
  requests are refused. No delete, archive or settings tools.

### Fixed

- A record's category must be one of the app's categories; an unknown one is
  rejected instead of saved where no category view shows it.
- Reminder due dates must be real dates (as every other date already was),
  and an edit can no longer blank a reminder's title.
- Scanning a document no longer proposes a reminder with an impossible or
  missing date (the bulk importer already skipped those).

## [0.1.0] - 2026-11-02

The first public release.

### Added

- Pets of any species, with the profile facts a vet intake form asks for:
  breed, sex (a fixed set, worded for the species), colour, birthday, gotcha
  day, microchip and weight, plus a life stage from the AAHA/AAFP guidelines.
- Medical records with Markdown notes, cost, itemised line items, custom
  fields, the treating clinic, and attachments that open inline (PDFs and
  images, including iPhone HEIC photos).
- Ongoing care: food, preventatives, medications and supplements, with what
  each preventative protects against.
- Allergies shown as a medical alert on the pet's profile.
- Reminders, a "next up" on every pet, and a month and agenda calendar.
- Shared contacts (vets, emergency, specialists, groomers, daycare and
  boarding) with hours and facility requirements, and a care team per pet.
- Costs by category and pet, insurance and wellness plans, and per-term
  insurance value: premium paid in, payouts, and the break-even bill.
- Optional AI document reading with your own Anthropic API key (stored
  encrypted): scan one document into a pre-filled record, read a facility's
  website into a contact, or bulk-import a stack of documents that are split
  into visits per pet, de-duplicated, and saved only after review.
- Archiving a pet as passed or rehomed, editable and reversible.
- Pet portrait photos with crop and rotate, and a family portrait on Home.
- Phone and desktop layouts, light and dark themes, installable web app.
- A REST API under `/api` for everything the UI does.
- A fictional demo household, enabled with `FURFILES_SEED_DEMO=1` on an empty
  database, dated relative to the day it is seeded.
- `FURFILES_OWNER_NAME` to name the household owner on a new install.
- Docker Compose file, CI for the backend and frontend test suites, and a
  container image published to `ghcr.io/jacob-lasky/furfiles`.

### Fixed

- Page headers on a phone wrap their buttons below the title instead of
  squeezing the title into a narrow column (Calendar).
- A new install no longer names its household owner after the author.

[Unreleased]: https://github.com/Jacob-Lasky/furfiles/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Jacob-Lasky/furfiles/compare/v0.2.0...v0.2.1
[0.2.0]: https://github.com/Jacob-Lasky/furfiles/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/Jacob-Lasky/furfiles/releases/tag/v0.1.0
