# opn.onl — Implementation & Go-To-Market Plan: Five New Features

## Context

opn.onl is a Rust (Axum + SeaORM + PostgreSQL, optional Redis cache, click buffer, WebSocket) URL shortener with a React 19 + TypeScript + Vite + Tailwind frontend. This plan adds five features — **Smart Conditional Routing**, **Link-in-Bio**, **Branded QR Codes**, **Burn-After-Reading**, and **Safe-Link Interstitial + Reputation** — and the marketing/SEO surface to announce them.

The plan is grounded in the actual code, not the summary. Key verified facts that shape every decision below:

- **The redirect hot path is dual-route** (`redirect_link`, `backend/src/handlers/links.rs:860-996`). It tries Redis first (`get_link`, lines 868-915), and **the cached fast path deliberately skips any link with `max_clicks` set or a password** (line 871: `if !cached.has_password && cached.max_clicks.is_none()`). Those links always fall through to the DB path (lines 919-995), which enforces `is_active()` + a buffered-count overshoot guard (lines 942-946). This is the single most important integration seam: **anything that must be evaluated per-request (routing rules, burn, interstitial) must either live behind the `max_clicks`/password cache-skip or invalidate/avoid the cache.**
- **`CachedLink` (`backend/src/utils/cache.rs:6-46`) only carries** `id, original_url, has_password, expires_at, starts_at, max_clicks, click_count, user_id`. It does **not** carry routing rules, burn flags, or interstitial flags. New per-request behavior must force the DB path or extend this struct + its JSON (de)serialization.
- **`links::Model` (`backend/src/entity/links.rs:5-27`)** already has `expires_at`, `starts_at`, `max_clicks`, `click_count`, plus `is_active()`/`inactive_reason()` helpers (lines 100-156). **Burn-after-reading is mostly `max_clicks` with a flag.**
- **GeoIP + UA parsing already exist and are already called on every click** (`record_click_buffered`, lines 999-1043, calls `lookup_ip` + `parse_user_agent`). `lookup_ip` returns `GeoLocation { country, country_code, city, region, ... }`; `parse_user_agent` returns `UserAgentInfo { browser, os, device }` where `os ∈ {iOS, Android, Windows…, macOS, Linux, Chrome OS}` and `device ∈ {Mobile, Tablet, Desktop, Bot}` (`backend/src/utils/geoip.rs:108-185`). **Routing reuses these verbatim.**
- **A preview/interstitial surface already exists**: backend `preview_link` (`links.rs:799-842`, response struct `LinkPreviewResponse`, **no auth**, strips trailing `+`), route `GET /:code/preview` (`main.rs:380`), frontend `Preview.tsx` with a warnings block (lines 159-175) and `noIndex`. **Feature 5 extends this, it does not build new.**
- **Route order matters**: `/:code` redirect is registered **last** (`main.rs:381`, comment "must be last") after `/:code/verify` and `/:code/preview`. The catch-all `:code+`/`:code` on the frontend (`App.tsx:83-84`) similarly swallows unknown paths. **Link-in-bio routes must be registered before these.**
- **Feature-flag pattern**: `get_app_settings` (`backend/src/handlers/auth.rs:630-669`) reads `ENABLE_*` via `std::env::var(...).unwrap_or("default").parse::<bool>()` and serializes booleans in `AppSettingsResponse` (lines 610-619). Frontend reads `API_ENDPOINTS.appSettings` (`/auth/settings`). Defaults vary: `ENABLE_ACCOUNT_DELETION` defaults `false`, `ENABLE_CUSTOM_ALIASES` defaults `true`.
- **`users::Model` (`backend/src/entity/users.rs:6-29`) already has** `display_name, bio, website, avatar_url, location`. **Link-in-bio reuses these for the profile header** — no new profile columns needed, only a `username` + opt-in flag.
- **QR**: `get_qr_code` (`links.rs:1141-1206`) is the only QR path, auth-gated, renders fixed B/W `Luma<u8>` PNG via `qrcode::QrCode::new` (defaults to `EcLevel::M`). `qrcode 0.14.1` + `image 0.25.9` are already deps; `qrcode` default features include `image` + `svg`. **Brand color + SVG need no new crates; logo overlay composites `logo.png`; PDF needs one new crate.**
- **Migrations** are sequential, latest `m20220101_000019_add_token_version`. Next slot is **`m20220101_000020`**. Column-add style is `Table::alter().table(Links::Table).add_column_if_not_exists(ColumnDef::new(...))` (`m20220101_000018_add_link_pinned.rs`).

Design system in play: editorial-technical, tokens `primary` (cobalt `#2f37d8`), `ink`, `muted`, `line`; Bricolage Grotesque + Hanken Grotesque; redesigned dashboard under `frontend/src/components/dashboard/`.

---

## Feature 1 — Smart Conditional Routing

> One short link → different destinations by device (iOS/Android/desktop), country/geo, language, and/or time window; optional weighted A/B split. Reuses existing GeoIP + UA parsing.

### Data model / migration — `m20220101_000020_create_routing_rules`

A new child table (ordered rule list per link) rather than JSON-in-column, so rules are queryable, indexable, and editable individually.

```
routing_rules
  id            i32 PK
  link_id       i32 NOT NULL  FK -> links.id ON DELETE CASCADE
  priority      i32 NOT NULL DEFAULT 0      -- lower = evaluated first
  -- match conditions (all NULL = "always matches", used as the catch-all/default)
  match_device  varchar NULL   -- "Mobile" | "Tablet" | "Desktop"  (matches UserAgentInfo.device)
  match_os      varchar NULL   -- "iOS" | "Android" | "Windows*" | "macOS" | "Linux"  (matches UserAgentInfo.os)
  match_country varchar NULL   -- ISO code, matches GeoLocation.country_code (uppercased)
  match_lang    varchar NULL   -- BCP-47 primary subtag, e.g. "en","de" (from Accept-Language)
  time_start    time   NULL    -- UTC window start (HH:MM)
  time_end      time   NULL    -- UTC window end
  -- destination
  destination_url text NOT NULL
  weight        i32 NOT NULL DEFAULT 1      -- A/B: relative weight among rules tied at same priority+conditions
  created_at    timestamp NOT NULL DEFAULT now()
INDEX idx_routing_rules_link (link_id, priority)
```

- The link's own `original_url` remains the **final fallback** — if no rule matches, redirect there. No migration to `links` needed.
- Entity: new `backend/src/entity/routing_rules.rs` (mirror `links.rs` `DeriveEntityModel`, `belongs_to` link). Register in `backend/src/entity/mod.rs`. Add `has_many = routing_rules` relation on `links::Model`.
- Add migration module to `backend/migration/src/lib.rs` (`mod` + `Box::new(...)` in the `migrations()` vec).

### Backend changes

1. **Evaluation helper** — new `backend/src/utils/routing.rs`:
   ```rust
   pub fn resolve_destination(
       rules: &[routing_rules::Model],
       ua: &UserAgentInfo,        // from parse_user_agent (already computed at redirect)
       geo: &GeoLocation,         // from lookup_ip (already computed at redirect)
       accept_language: Option<&str>,
       now_utc: chrono::NaiveTime,
       fallback: &str,            // link.original_url
   ) -> String
   ```
   - Sort rules by `priority` asc. Iterate; a rule matches when **every non-NULL condition** matches (device/os case-insensitive equals; `match_os` "Windows*" prefix-matches "Windows 10/11"; country compares against `geo.country_code` uppercased; lang compares primary subtag of the first `Accept-Language` entry; time window: if `time_start <= time_end` it's `[start,end]`, else it wraps midnight).
   - **A/B split**: among the matched rules sharing the same `priority` *and* identical condition tuple, pick one weighted-randomly by `weight` (use `rand::thread_rng`, already a dep, seeded per-request — *not* sticky; document that A/B is per-click, not per-visitor in v1). First fully-matching rule (or weighted pick among an equal-priority group) wins; else return `fallback`.
   - Pure function → unit-testable with synthetic `UserAgentInfo`/`GeoLocation`.

2. **`redirect_link` (`links.rs:860`)** integration — **must bypass the Redis fast path** because routing depends on per-request UA/geo/lang/time that `CachedLink` doesn't carry:
   - Cleanest, lowest-risk approach: **treat "has routing rules" like "has max_clicks"** — i.e. exclude routed links from the cached fast path so they always hit the DB branch. Add a boolean to `CachedLink` (`has_routing_rules: bool`) populated when caching (line 967-977) and gate the fast path at line 871: `if !cached.has_password && cached.max_clicks.is_none() && !cached.has_routing_rules`. This reuses the existing skip mechanism and keeps the hot path for the 99% of plain links untouched.
   - In the DB branch (after `is_active()` check, before building the redirect at line 992): if the link has routing rules, load them (`routing_rules::Entity::find().filter(link_id).order_by_asc(priority).all()`), compute `ua_info`/`geo` (already computed inside `record_click_buffered`; refactor so the redirect computes them once and passes them in, or recompute cheaply — UA parse is a string scan, GeoIP is an mmap lookup), read `Accept-Language` header, call `resolve_destination`, and `Redirect::temporary(&destination)`.
   - **Do not cache routed links' resolved destination** (it's per-request). Keep them out of `set_link`.
   - **Feature flag**: gate the *creation/editing* of rules behind `ENABLE_CONDITIONAL_ROUTING` (default `false`). At redirect time, if the flag is off, simply skip rule evaluation and fall back to `original_url` — existing routed links degrade to plain redirects, never break.

3. **CRUD endpoints** (new, in `links.rs`, registered in `main.rs` near line 322 with the other `/links/:id/...` routes):
   - `GET /links/:id/rules` → list (auth + ownership check, reuse the org/owner check block from `get_qr_code` lines 1163-1182).
   - `PUT /links/:id/rules` → replace-all (accept `Vec<RoutingRuleInput>`, validate each destination via existing `validate_url` (`links.rs:116`), enforce a max rule count e.g. 20, delete-then-insert in a transaction). On success **invalidate the Redis cache** for that code so the `has_routing_rules` flag refreshes.
   - Validate destinations against `check_blocked` (`links.rs:23`) so routing can't bypass the blocklist.

### Frontend changes

- **`EditModal.tsx`** (`frontend/src/components/dashboard/EditModal.tsx`): add a collapsible "Smart routing" section (shown only when `appSettings.conditional_routing_enabled`). A rule editor: rows of `{ device select, OS select, country input, language input, time window, destination URL, weight }`, add/remove, drag-to-reorder (priority). On save, `PUT /links/:id/rules`.
- **`config/api.ts`**: add `linkRules: (id:number) => \`${API_BASE_URL}/links/${id}/rules\``.
- **Dashboard.tsx**: show a small "routed" badge on links that have rules (the `LinkResponse` can gain a `routing_rule_count: i32`).
- Keep create-link (`CreateLinkRequest`) unchanged for v1 — rules are added via edit (avoids bloating the create form).

### Defaults & safety

- `ENABLE_CONDITIONAL_ROUTING=false` by default. When off: no UI, and redirect ignores any rules → plain `original_url`. **Existing links and the fast path are never affected.**
- Routed links opt out of the Redis fast path (same mechanism as `max_clicks`), so correctness is guaranteed; the perf cost is one extra indexed query only for links that actually have rules.
- A/B is per-click (documented); the fallback `original_url` guarantees every request resolves to *something* even if all rules are malformed.

### Edge cases

- No rule matches → `original_url`. All conditions NULL → matches everything (explicit default rule).
- Bots (`device == "Bot"`): allow a rule to target them, but default behavior sends them to `original_url` (don't A/B-pollute on crawlers — optionally skip A/B randomization for `Bot` and pick the first match deterministically).
- Private/local IP → `GeoLocation::default()` (country None) so country rules simply don't match → fallback. Consistent with existing GeoIP behavior.
- Missing `Accept-Language` → lang rules don't match.
- Weight sum 0 or single rule → deterministic pick.
- Destination later blocked → caught by `check_blocked` at edit time; at redirect time the link-level `check_blocked` (line 936) still guards the *original_url* but **not** rule destinations — add a `check_blocked` on the resolved destination before redirecting (important security note).

### Tests to add

- `utils/routing.rs` unit tests: device-only match; OS "Windows*" prefix; country match via `country_code`; lang subtag extraction; time window incl. midnight wrap; weighted split distribution (statistical, seeded); fallback when nothing matches; priority ordering.
- Handler test: `PUT` then `GET` round-trip; ownership 403; destination validation rejects `javascript:`; cache invalidation after `PUT`.
- Redirect integration: a link with a Mobile→X rule returns X for an iPhone UA and `original_url` for desktop; flag-off path ignores rules.

**Effort: L**

---

## Feature 2 — Link-in-Bio Page (opt-in, OFF by default)

> A hosted public profile aggregating a user's links — privacy-first linktree. MUST be optional and disabled by default; no public page unless the user turns it on.

### Data model / migration — `m20220101_000021_add_link_in_bio`

Reuse existing `users` profile columns (`display_name, bio, website, avatar_url, location`). Add only the bio-page controls:

```
ALTER users ADD COLUMN bio_username   varchar NULL UNIQUE   -- public slug; NULL = never claimed
ALTER users ADD COLUMN bio_enabled    boolean NOT NULL DEFAULT false  -- the opt-in switch
ALTER users ADD COLUMN bio_theme      varchar NULL          -- "auto"|"light"|"dark"|brand token, optional
```

Per-link "show on bio" + ordering:

```
ALTER links ADD COLUMN bio_visible    boolean NOT NULL DEFAULT false  -- link opt-in to appear
ALTER links ADD COLUMN bio_position   i32 NULL                         -- manual ordering
ALTER links ADD COLUMN bio_label      varchar NULL                     -- display label override (else title)
```

- `bio_username UNIQUE` + partial index. Mirror all new fields in `users::Model` / `links::Model`.
- This is **two migrations or one combined** — recommend one (`m20220101_000020`/`...0021`) covering both tables.

### Backend changes

1. **Public read endpoint** (new, **no auth**, in a new `backend/src/handlers/bio.rs`):
   - `GET /api/bio/:username` → returns profile (display_name, bio, avatar_url, website, location) + the user's `bio_visible` links ordered by `bio_position` (`code`, `short_url`, `bio_label`/`title`, `click_count` optional).
   - **Hard gate**: only returns data when `bio_enabled == true` **and** `ENABLE_LINK_IN_BIO == true`. Otherwise `404` (not 403 — don't reveal that a username exists). This is the privacy contract.
   - Never expose email, notes, password state, or non-visible links.
2. **Owner management endpoints** (auth'd):
   - `PUT /auth/bio` → set `bio_username` (validate slug: `^[a-z0-9_-]{3,30}$`, reserved-word blocklist incl. existing route names `dashboard/settings/docs/...`, uniqueness check), `bio_enabled`, `bio_theme`.
   - Per-link `bio_visible`/`bio_position`/`bio_label` editable via existing `UpdateLinkRequest` (`links.rs:338`) — add those optional fields and handle in `update_link`.
3. **App settings**: add `link_in_bio_enabled: bool` to `AppSettingsResponse` (`auth.rs:610`) populated from `ENABLE_LINK_IN_BIO` (default `false`).
4. **Optional dynamic sitemap** (only if indexable bio pages are wanted): `GET /sitemap-bio.xml` listing `bio_enabled` users — gate behind the same flag; otherwise omit.
5. **No Redis cache interaction** — bio is its own endpoint, never touches `redirect_link`.

### Frontend changes

- **New public route** in `App.tsx` — register **above** the `:code+`/`:code` catch-alls (lines 83-84) to avoid being swallowed. Recommended shape **`/@:username`** (distinctive, unmistakably a profile, low collision risk). Route renders new `frontend/src/pages/Bio.tsx`.
- **`Bio.tsx`**: fetches `GET /api/bio/:username`; on 404 renders the standard `NotFound`. On success renders avatar + display_name + bio + a vertical stack of link buttons (editorial-technical styling — `primary`/`ink`/`line` tokens). Renders `<SEO/>` with a new `ProfilePage` schema type (add `'ProfilePage'` to the `schemaType` union in `SEO.tsx:11`) and per-page OG (title = display_name, description = bio). Default `noIndex` unless the user opted into indexing (see safety).
- **Settings.tsx**: a "Public profile (Link-in-Bio)" card, hidden entirely unless `appSettings.link_in_bio_enabled`. Contains: enable toggle (default off), username claim field with live availability check, theme, and a link-picker / reorder list to choose which links appear (`bio_visible`/`bio_position`).
- **`config/api.ts`**: `bioPublic: (u:string) => \`${API_BASE_URL}/api/bio/${u}\``, `bioSettings: \`${API_BASE_URL}/auth/bio\``.

### Defaults & safety (constraint: OFF by default)

- `ENABLE_LINK_IN_BIO=false` (instance-level) **and** `bio_enabled=false` (per-user) — **both** must be true for any page to exist. Default state = no public page, the UI card is hidden.
- Per-link `bio_visible=false` default → opting into a profile does not expose any link until explicitly added.
- `robots.txt` (`frontend/public/robots.txt`): add `Disallow: /@` so profiles are not crawled by default. If/when a user opts into indexing, rely on per-page Helmet `noIndex={false}` + the dynamic sitemap; default remains noindex.
- Public endpoint returns `404` for disabled/instance-off to avoid username enumeration.

### Edge cases

- Username collision with reserved routes (`docs`, `pricing`, `@admin`, etc.) → reject at claim time.
- User disables `bio_enabled` after sharing → endpoint immediately 404s (no stale public page).
- Deleted/expired links that are `bio_visible` → filter out inactive links (`is_active()`) from the public response.
- Soft-deleted user (`deleted_at`) → 404.
- Account deletion must null `bio_username` so it can be reclaimed.

### Tests to add

- Public endpoint: enabled+visible returns links; `bio_enabled=false` → 404; flag off → 404; only `bio_visible` links appear; inactive links excluded; no PII fields in JSON.
- Slug validation: reserved words, format, uniqueness.
- Settings round-trip; per-link `bio_visible` toggle via `update_link`.
- Frontend: `Bio.test.tsx` renders profile + buttons; 404 path; route registered before catch-all.

**Effort: L**

---

## Feature 3 — Branded QR Codes ("if possible, try")

> Extend existing QR with brand color + center logo (`frontend/public/logo.png`), nicer export (SVG; PDF if feasible). Reuse existing QR + logo.

### Feasibility verdict (confirmed against code + crate features)

- `qrcode 0.14.1` (`Cargo.toml`) ships `image` + `svg` features **by default** → brand color (`.dark_color`/`.light_color`), Rgba rendering, and SVG `String` output need **zero new crates**.
- `image 0.25.9` (`Cargo.toml`) provides `imageops::{overlay, resize}` for compositing `logo.png` → logo overlay needs **zero new crates**.
- `base64` is already a dep → inlining the logo into branded SVG needs **zero new crates**.
- **PDF is the only piece needing a new crate** (`image`/`qrcode` cannot emit PDF). Recommendation: **ship PNG + SVG in v1, treat PDF as optional follow-up** behind `printpdf`.

### Data model / migration

**None.** Branding is **stateless per-request** via query params on the existing `GET /links/:id/qr`. (Optional future `m20220101_000022` could add `qr_brand_color`/`qr_logo_enabled` to `links` for persisted defaults — **deferred**.)

### Backend changes — `get_qr_code` (`links.rs:1141-1206`)

1. Add `Query<QrOptions>` param: `{ color: Option<String>, bg: Option<String>, logo: Option<bool>, format: Option<String> /* png|svg|pdf */, size: Option<u32> }`.
2. **Parse hex defensively** (helper `parse_hex(&str) -> Option<[u8;3]>`); on any parse failure fall back to black/white → **byte-identical to today when no/invalid params** (protects all existing callers and current `QRModal`).
3. **EC level**: replace `QrCode::new(url.as_bytes())` (line 1186) with `QrCode::with_error_correction_level(url.as_bytes(), EcLevel::H)` **whenever a logo is requested** (≈30% redundancy survives occlusion); keep `EcLevel::M` for plain codes (denser).
4. **PNG branch** (replace lines 1191-1196): render `qr.render::<image::Rgba<u8>>().dark_color(...).light_color(...).min_dimensions(clamp(size,256..=1024), …).quiet_zone(true).build()`. If `logo`, composite: load logo via a `Lazy<Option<DynamicImage>>` (copy the `GEOIP_READER` path-probe + graceful-`None` pattern from `geoip.rs:20-45`), **resize to ~20-22% of QR width letterboxed into a square** (logo is 498×256, ~2:1 — must not stretch), draw a solid `bg`-colored rounded backplate to clear modules under it, `imageops::overlay` centered. Encode PNG exactly as today.
5. **SVG branch** (`format=svg`): `qr.render::<qrcode::render::svg::Color>().dark_color(...).light_color(...).build()` → `String`, return `Content-Type: image/svg+xml`. For logo-on-SVG, string-splice an `<image href="data:image/png;base64,…">` (base64 of the cached logo) centered before `</svg>`. Plain SVG needs no post-processing.
6. **Logo asset provisioning** (the critical gap — backend `Dockerfile` runtime stage copies only the binary + 2 scripts, **not** `logo.png`): **embed via `include_bytes!`**. Copy `frontend/public/logo.png` → `backend/assets/qr-logo.png`, load once with `static QR_LOGO: Lazy<Option<DynamicImage>> = Lazy::new(|| image::load_from_memory(include_bytes!("../../assets/qr-logo.png")).ok())`. Zero runtime FS dependency, no Dockerfile change, always present, degrades to plain QR if ever absent. (Recommend a **square monochrome mark** variant for cleaner center legibility — open question for owner.)
7. **Kill-switch flag**: `ENABLE_QR_BRANDING` default **`true`** (non-destructive, unlike the off-by-default features). Surface as `qr_branding_enabled` in `AppSettingsResponse`. When `false`, handler ignores `color/logo/format` and serves the plain PNG.

### Frontend changes — `QRModal.tsx`

- Add state: `brandColor` (default `primary` `#2f37d8`), `logoOn` (bool), `format` (`'png'|'svg'`). Render a color picker, a logo on/off toggle, and a format dropdown — **hidden when `appSettings.qr_branding_enabled === false`**.
- Build fetch URL by appending params to `linkQr(id)`: `?color=${enc(color)}&logo=${logoOn?1:0}&format=${format}`. Extend the `config/api.ts` `linkQr` builder (line 44) to accept an optional options object, or build the query string inline.
- Re-fetch on option change (extend the `useEffect` deps). **Fix the download extension** (currently hardcoded `.png`, `QRModal.tsx:44`): `qr-${link.code}.${format}`. SVG blobs already preview via the existing `<img src=objectURL>`.

### Defaults & safety

- No params → identical to today. Branding is opt-in per request. `ENABLE_QR_BRANDING=true` by default (kill-switch only). Logo self-disables to plain QR when `QR_LOGO` is `None`.

### Edge cases

- Low-contrast user colors (light-on-light) → unscannable: validate hex, optionally enforce a min luminance delta, else document. Clamp `size` (DoS guard) and cache the decoded/resized logo in the `Lazy` static (decode once).
- SVG + logo without the splice → unbranded SVG; set FE expectation or do the splice.
- Logo aspect ratio → letterbox into square safe area; never full-width paste (occludes too many modules even at EC-H).

### Tests to add

- Handler: no params → PNG, `image/png`, non-empty (regression-lock the plain path); `color=...` → still valid PNG; `format=svg` → `image/svg+xml`, body starts `<svg`; `logo=1` → larger PNG, no panic when logo missing (None path); invalid hex → falls back; `ENABLE_QR_BRANDING=false` ignores params.
- A scan-decode test (decode the generated PNG back to the URL) for plain and EC-H+logo to assert scannability.

**Effort: M** (PNG color + SVG + logo); PDF a separate **S/M** follow-up.

---

## Feature 4 — Burn After Reading

> One-time / self-destruct link: works once (or N times) then gone/disabled. Build on existing expiry + `max_clicks`.

### Data model / migration — `m20220101_000020_add_burn_after_reading`

Burn is **`max_clicks` semantics + an intent flag + a terminal state**. Add to `links`:

```
ALTER links ADD COLUMN burn_after_reading boolean NOT NULL DEFAULT false  -- intent: self-destruct on cap
ALTER links ADD COLUMN burned_at          timestamp NULL                   -- set when terminal
```

- `max_clicks` already carries the "N" (N=1 for classic one-time). `burn_after_reading=true` + `max_clicks=N` means: once `click_count >= N`, the link is **permanently dead** (not merely inactive) and shows a "this link has been burned" message instead of the generic max-clicks message.
- Mirror both fields in `links::Model`; extend `is_active()`/`inactive_reason()` (`links.rs:100-156`) to treat `burned_at.is_some()` as inactive with reason "Link has been burned (one-time use)".

### Backend changes

1. **`CreateLinkRequest` (`links.rs:322`) + `UpdateLinkRequest` (`links.rs:338`)**: add `burn_after_reading: Option<bool>` (and reuse existing `max_clicks`; default N=1 when burn is set without `max_clicks`). `create_link`/`update_link` persist it.
2. **Cache correctness — this is why it's clean**: links with `max_clicks` set **already bypass the Redis fast path** (`redirect_link` line 871 / cache write line 965). Burn links *always* have `max_clicks` (≥1), so **they always take the DB branch** — no `CachedLink` changes needed, and the buffered-overshoot guard (lines 942-946) already prevents a click burst from exceeding the cap during the buffer window. This is the decisive reuse.
3. **Terminal transition**: in the DB branch of `redirect_link`, after the existing `max_clicks` overshoot check (lines 942-946), when the cap is reached **and** `burn_after_reading`, perform the redirect for the *final* allowed click, then mark `burned_at = now()` (best-effort, after recording the click). For subsequent requests, the `max_clicks` guard already returns `GONE`; `burned_at` lets us return the *burn* message + lets the dashboard show "Burned".
   - Set `burned_at` via an `UPDATE links SET burned_at = now() WHERE id = ? AND burned_at IS NULL` once the buffered+stored count reaches the cap (idempotent).
4. **Optional flag**: `ENABLE_BURN_AFTER_READING` default **`false`**. When off, the create/edit UI hides the option and the backend ignores `burn_after_reading` (link behaves as a normal `max_clicks` link, or unlimited if no cap).

### Frontend changes

- **Create form (Dashboard.tsx create section) + `EditModal.tsx`**: add a "Burn after reading" toggle (shown only when `appSettings.burn_after_reading_enabled`) with an optional "uses allowed" number (defaults 1). Wire into `CreateLinkRequest`/`UpdateLinkRequest`.
- **Dashboard link list / `MiniStats.tsx`**: show a "🔥 burned" / "one-time" badge; `LinkResponse` gains `burn_after_reading: bool` + `burned_at: Option<String>`.
- **Redirect UX**: when a burned link is visited, the existing `GONE` page copy should read "This one-time link has already been opened." (Redirect.tsx / NotFound handling).

### Defaults & safety

- `ENABLE_BURN_AFTER_READING=false` default. Existing links unaffected (`burn_after_reading=false`, `burned_at=NULL`). Reuses the proven `max_clicks` cache-skip + overshoot guard → **no new fast-path risk**.

### Edge cases

- Concurrency: two simultaneous clicks on a `max_clicks=1` burn link — the buffered-count guard (`click_buffer.pending_count`, line 943) already prevents both from succeeding; only the first redirects, the second gets `GONE`. (Document that under extreme races at most `N` succeed, never `N+1`.)
- Burn + password: password links also bypass the cache; the password check runs first, then the cap — order is correct (don't burn on a failed password attempt: only record/count on success, matching existing `verify_link_password` behavior).
- Burn + routing rules: a burned link should not evaluate rules → the `max_clicks`/`burned_at` inactive check short-circuits before routing.
- Setting `burn_after_reading=true` with no `max_clicks` → default N=1.

### Tests to add

- `is_active()`/`inactive_reason()` with `burned_at` set, and with `burn_after_reading` + `click_count == max_clicks`.
- Redirect integration: N=1 link redirects once (302) then returns 410 with burn message; `burned_at` gets set; cache never serves it (no `set_link`).
- Concurrency: simulate `pending_count` ≥ cap → second click 410.
- Flag-off: `burn_after_reading` ignored.

**Effort: S/M** (mostly leverages `max_clicks`).

---

## Feature 5 — Safe-Link Interstitial + Reputation Check (OFF by default)

> Optional "You're leaving to X — looks safe ✓" preview before redirect, with a reputation/safety signal. MUST be OFF by default, applied only where it makes sense, and MUST NOT break the fast redirect or existing links.

### Data model / migration — `m20220101_000020_add_safe_link` (optional column)

Interstitial is **opt-in per link** plus an instance flag. Minimal addition to `links`:

```
ALTER links ADD COLUMN safe_link_interstitial boolean NOT NULL DEFAULT false  -- per-link opt-in
```

Plus a small reputation cache table to avoid hammering an external API:

```
url_reputation
  id            i32 PK
  domain        varchar NOT NULL UNIQUE
  verdict       varchar NOT NULL   -- "safe" | "suspicious" | "malicious" | "unknown"
  source        varchar NOT NULL   -- "internal_blocklist" | "external_api"
  checked_at    timestamp NOT NULL
INDEX idx_url_reputation_domain (domain)
```

- Mirror `safe_link_interstitial` in `links::Model`. The reputation table is optional in v1 — could start with **internal blocklist reuse only** (`blocked_domains`) and add external later.

### Backend changes

**The hard constraint: do not touch the fast `/:code` redirect path.** The interstitial is reached via the **existing `+` suffix preview route** (`/:code/preview` → `preview_link`, `links.rs:799`), not the redirect. So the fast path stays byte-identical.

1. **Extend `preview_link` response** (`LinkPreviewResponse`): add `reputation: { verdict, source }` and `interstitial_enabled: bool`. Compute reputation:
   - Always check the **existing internal blocklist** (`check_blocked` / `blocked_domains`, `links.rs:23-60`) → if blocked, `verdict="malicious"`.
   - If `ENABLE_SAFE_LINK_INTERSTITIAL` and `REPUTATION_API_URL` is set: look up `url_reputation` cache by domain; if stale/missing, call the external API (with `REPUTATION_API_KEY`), store the verdict, default `unknown` on timeout/error. **Never block the preview on a slow API** — short timeout, fail-open to `unknown` with a neutral message.
2. **New optional endpoint** `GET /:code/safety` (or fold into `preview`) returning just `{ domain, verdict, source }` for lightweight polling. Register **before** `/:code` in `main.rs`.
3. **Auto-interstitial logic** (where it makes sense): the interstitial is shown when **any** of: the link has `safe_link_interstitial=true`, OR the destination is flagged `suspicious`/`malicious` by reputation (force the warning even on a normal redirect — a `GONE`/redirect-to-preview), OR the visitor explicitly used the `+` suffix. **Default normal links with `safe` verdict redirect instantly — no interstitial, no added latency.**
   - To force-interstitial a flagged destination *without* slowing every redirect: the reputation verdict can be **cached in Redis alongside the link** (extend `CachedLink` with `reputation_verdict: Option<String>`), so the fast path can cheaply check "is this flagged?" and 302 to `/:code/preview` only for the rare flagged case. Safe/unknown links keep the instant path. This keeps the constraint intact.
4. **Flags**: `ENABLE_SAFE_LINK_INTERSTITIAL` default **`false`**; `REPUTATION_API_URL` / `REPUTATION_API_KEY` (empty default). Surface `safe_link_interstitial_enabled: bool` in `AppSettingsResponse`. When off: `preview_link` still works (it already does) but omits reputation; no link ever auto-interstitials.

### Frontend changes — extend existing `Preview.tsx`

- Add a **reputation block** near the warnings (`Preview.tsx:159-175`): when `reputation.verdict === 'safe'` render a green "You're leaving to **{domain}** — looks safe ✓"; `suspicious`/`malicious` render an amber/red caution with the source; `unknown` renders a neutral "We couldn't verify this destination." Reuse the existing warnings markup + lucide icons (`AlertTriangle`, `Shield`).
- Extend the `LinkPreview` interface (`Preview.tsx:8-17`) with `reputation`. Keep `noIndex`.
- **Per-link toggle** in `EditModal.tsx` / create form: "Show safety interstitial before redirect" (hidden unless `appSettings.safe_link_interstitial_enabled`). Wire `safe_link_interstitial` into `CreateLinkRequest`/`UpdateLinkRequest`.
- `config/api.ts`: `linkSafety: (code:string) => \`${API_BASE_URL}/${code}/safety\`` (optional).

### Defaults & safety (constraint: OFF by default, don't break fast path / existing links)

- `ENABLE_SAFE_LINK_INTERSTITIAL=false`. When off: zero behavior change — `/:code` redirects instantly as today, `preview_link` returns its current shape sans reputation, **existing links untouched**.
- The interstitial is reached via the **already-existing** `+`/preview surface, so the hot `/:code` path is not modified for normal `safe`/`unknown` links.
- Only **flagged** (suspicious/malicious) or **explicitly opted-in** links ever interstitial — "applied only where it makes sense."
- Reputation lookups fail-open (`unknown`, neutral copy), short timeout, Redis/DB-cached per domain → no DoS, no latency on the common path.

### Edge cases

- External API down/slow → `unknown`, neutral message, never blocks.
- Reputation cache staleness → TTL on `checked_at`; re-check on expiry.
- A link both password-protected and interstitial → preview shows the password warning + reputation; the actual redirect still enforces the password.
- Don't double-count clicks: the preview/safety endpoints must **not** call `record_click_buffered` (only the real redirect does — confirmed `preview_link` doesn't today).
- Internal blocklist already returns `GONE` at redirect (line 936) — interstitial for `malicious` is belt-and-suspenders, not a replacement.

### Tests to add

- `preview_link` returns `reputation` when flag on; omits when off; blocklisted domain → `malicious`; external API timeout → `unknown` (mock).
- Fast `/:code` redirect for a `safe` link is unchanged (302, no preview detour) — regression lock.
- Flagged link in cache → fast path 302s to `/:code/preview` (only when flag on).
- Frontend `Preview.test.tsx`: renders safe/suspicious/unknown states.

**Effort: M** (extends existing Preview; external API integration is the variable cost).

---

## Cross-Cutting Concerns

### New environment flags (and where they must be declared — all five places)

For each flag: declare in **(1)** `.env.example`, **(2)** `backend/.env.example`, **(3)** `docker-compose.portainer.amd64.yml` + **(4)** the arm64 sibling (`ENABLE_X: ${ENABLE_X:-default}`, copy the pattern at `portainer.amd64.yml:89-95`), **(5)** `docker-compose.yml` backend `environment:` block (currently omits `ENABLE_*` — add them after the MAXMIND lines ~102). Read in backend via `std::env::var("ENABLE_X").unwrap_or_else(|_| "default".into()).parse::<bool>().unwrap_or(default)` and surface in `AppSettingsResponse` (`auth.rs:610-619`) + `get_app_settings` (`auth.rs:630-668`).

| Flag | Default | Surfaced as (AppSettings field) | Notes |
|------|---------|--------------------------------|-------|
| `ENABLE_CONDITIONAL_ROUTING` | `false` | `conditional_routing_enabled` | Gates rule CRUD + UI; off → redirect ignores rules |
| `ENABLE_LINK_IN_BIO` | `false` | `link_in_bio_enabled` | Instance gate; per-user `bio_enabled` also required |
| `ENABLE_QR_BRANDING` | **`true`** | `qr_branding_enabled` | Kill-switch only; branding is per-request, non-destructive |
| `ENABLE_BURN_AFTER_READING` | `false` | `burn_after_reading_enabled` | Off → `burn_after_reading` ignored |
| `ENABLE_SAFE_LINK_INTERSTITIAL` | `false` | `safe_link_interstitial_enabled` | Off → no reputation, no auto-interstitial |
| `REPUTATION_API_URL` | (empty) | — (not exposed) | External reputation source |
| `REPUTATION_API_KEY` | (empty) | — (secret, not exposed) | Never serialized to FE |

Frontend consumes `API_ENDPOINTS.appSettings` (already used in `Settings.tsx`/`Dashboard.tsx`) and gates every new control on the matching boolean.

### Cache-invalidation rules (Redis `link:{code}`)

- **`CachedLink` is the contract.** Today it carries no routing/burn/interstitial state. Decisions:
  - **Burn**: no change — burn links always have `max_clicks` → already cache-skipped.
  - **Routing**: add `has_routing_rules: bool` to `CachedLink` (+ `to_redis_value`/`from_redis_value`, `cache.rs:20-45`); set it on cache-write; gate the fast path on it. **Invalidate the code's cache key on any `PUT /links/:id/rules`.**
  - **Interstitial/reputation**: optionally add `reputation_verdict: Option<String>` so the fast path can cheaply force-interstitial flagged links; invalidate on reputation change. Safe/unknown stay on the instant path.
- **General rule**: any write that changes routing/burn/interstitial/reputation for a code must call `cache.invalidate_link(code)` (the `update_link` path should already invalidate on URL/expiry/`max_clicks` changes — extend it to cover the new fields). The existing redirect already invalidates after each counted click (lines 904-911), which keeps `click_count` honest for capped links.
- **Schema-version the cache value**: bump a `v` field in the JSON so a deploy that changes `CachedLink` shape doesn't mis-read old entries (stale entries simply fail `from_redis_value` → DB fallback, which is safe, but a version field makes it explicit).

### Rate-limiting / abuse notes

- **Routing**: cap rules per link (≤20); validate every destination via `validate_url` + `check_blocked` at write *and* resolve time (rule destinations otherwise bypass the blocklist — security-relevant).
- **Link-in-bio**: public endpoint is unauthenticated → add basic per-IP rate limiting and `404`-on-disabled to prevent username enumeration; never leak PII.
- **Branded QR**: clamp `size`, cache the decoded logo (decode once), keep the handler auth-gated (it already is) → branding is dashboard-only.
- **Burn**: the buffered-overshoot guard already bounds clicks to ≤ N under races.
- **Interstitial/reputation**: cache verdicts per domain (DB/Redis) + short external-API timeout + fail-open → no amplification, no added latency on the hot path.

### Backward compatibility

- Every new column is `NOT NULL DEFAULT false` / nullable → existing rows valid post-migration.
- Every new behavior is flag-gated; off-by-default flags mean **zero behavior change** for current deployments until explicitly enabled.
- `get_qr_code` with no params is **byte-identical** to today.
- The fast `/:code` redirect is unchanged for plain links; only routed/burn/flagged links take the DB branch (the latter already do for `max_clicks`/password).
- New routes (`/@:username`, `/links/:id/rules`, `/:code/safety`) are additive and registered **before** the `/:code` catch-all.

---

## Website + GitHub + SEO

Marketing copy lives in **hand-authored inline arrays per page** (no shared registry) — each must be edited independently. SEO is centralized in `SEO.tsx`. **`Features.tsx`, `Faq.tsx`, `Pricing.tsx` currently render no `<SEO/>`**, and `Faq.tsx` passes no `faqItems` → **no FAQ rich result is emitted today** (wiring it is a free SEO win).

### Exact files to edit

| File | Edit |
|------|------|
| `README.md` | Add 5 feature bullets to the feature list (lines 10-54); add env-flag rows to the **Link Management** table (197-204) for routing/burn/link-in-bio + a new **Safety** subsection for interstitial/reputation (234-239); document new endpoints (`/links/:id/rules`, `/api/bio/:username`, `/:code/safety`, QR query params) in the API Reference (258-279) |
| `.env.example` + `backend/.env.example` | Add all 7 flags/vars with `(default: …)` comments in the LINK MANAGEMENT / SECURITY blocks |
| `docker-compose.yml` | Add `ENABLE_*: ${…:-default}` to backend `environment:` (~after line 102) |
| `docker-compose.portainer.amd64.yml` + arm64 sibling | Add `ENABLE_*: ${…:-default}` (copy lines 89-95) |
| `backend/src/handlers/auth.rs` | Add 5 boolean fields to `AppSettingsResponse` (610-619) + populate in `get_app_settings` (630-668) |
| `frontend/src/components/SEO.tsx` | Add 5 entries to `featureList` (59-70); add `'ProfilePage'` to `schemaType` union (line 11) for the bio page |
| `frontend/src/pages/Home.tsx` | Add feature cards to the `features` array (54-61) — **phrase opt-in features as optional** (don't mimic the absolute "Every link ships with a QR" tone for off-by-default ones) |
| `frontend/src/pages/Features.tsx` | Add rich cards to `features` (9-73); **add a `<SEO/>` render** (currently absent) |
| `frontend/src/pages/Docs.tsx` | Add routing rule body fields + `/links/:id/rules`, burn (`burn_after_reading`/`max_clicks`), QR query params on `/links/:id/qr` (191-195), `/:code/safety`, bio endpoint; update env-var table (264-282) |
| `frontend/src/pages/Faq.tsx` | Add Q&A to `faqs` (6-91); **render `<SEO/>` and pass `faqItems`** to finally emit FAQPage JSON-LD; update the existing QR answer (says "PNG format", line ~46) to mention SVG |
| `frontend/src/pages/Pricing.tsx` | Append new features to plan feature lists (Free 11-21, Pro 31-41, Self-Hosted 51-62) |
| `frontend/src/App.tsx` | Register `/@:username` → `Bio.tsx` **above** `:code+`/`:code` (83-84) |
| `frontend/vite.config.ts` | Leave per-user bio pages out of `PRERENDER_ROUTES` (dynamic); optionally add a static `/link-in-bio` explainer route if one is built |
| `frontend/public/sitemap.xml` | Add any new **static** marketing route; per-user bio pages → dynamic `/sitemap-bio.xml` or omit |
| `frontend/public/robots.txt` | Add `Disallow: /@` (bio off-by-default privacy) |
| `frontend/index.html` | Refresh default description/keywords/OG (39-58) if headline positioning shifts |
| `frontend/src/components/Layout.tsx` | Only add nav/footer entries (33-38, 206-209) for features that are globally on (not bio, which is per-user) |

### Draft marketing copy (editorial-technical voice)

- **Smart routing** (Home/Features card): *"One link, many destinations. Route by device, country, language or time of day — send iPhone users to the App Store, everyone else to the web. Optional A/B split, measured in real time."*
- **Link-in-Bio** (Features card, opt-in framing): *"A profile page for your links — if you want one. Off by default, public only when you flip the switch. No tracking pixels, no third parties, just your links on your own page."*
- **Branded QR** (upgrade existing QR card): *"QR codes that look like you. Drop in your brand colour and logo, export as PNG or crisp SVG — ready for print, packaging or a slide."*
- **Burn-after-reading** (Features card): *"One-time links that delete themselves. Set it to open once — or N times — then it's gone. Built on the same expiry and click-cap engine you already trust."*
- **Safe-link interstitial** (Security card, opt-in framing): *"A heads-up before you leave. Optional, off by default: show a 'you're heading to X — looks safe ✓' check on the links where it matters, without slowing down the ones where it doesn't."*

### New FAQ Q&A (add to `Faq.tsx` `faqs`)

- **Features** — *"Can one short link go to different places?"* → "Yes. With smart routing you can send visitors to different destinations based on their device, country, language or the time of day, with an optional weighted A/B split. It reuses the same geolocation and device detection that powers your analytics."
- **Features** — *"Do you support one-time / self-destructing links?"* → "Yes — turn on burn-after-reading and the link works once (or a number of times you choose), then disables itself permanently."
- **Features** — *"What formats can I export QR codes in?"* → "PNG and SVG, optionally with your brand colour and logo in the centre." (Replaces the current PNG-only answer.)
- **Security & Privacy** — *"Does opn.onl warn me before leaving to another site?"* → "It can. The safe-link interstitial is optional and off by default; when enabled it shows a destination preview with a reputation check before redirecting. Your normal links stay instant."
- **Security & Privacy** — *"Is my link-in-bio page public?"* → "Only if you opt in. The feature is off by default at both the server and the account level — no page exists until you create one and choose which links appear."

### Structured-data / SEO additions

- **`SoftwareApplication.featureList`** (`SEO.tsx:59-70`): append `'Smart conditional routing'`, `'Branded QR codes (SVG export)'`, `'One-time burn-after-reading links'`, `'Optional safe-link interstitial'`, `'Opt-in link-in-bio profile'` — keep in sync with the visible grids.
- **FAQPage**: render `<SEO faqItems={...} />` on `Faq.tsx` (currently missing) so the new Q&A produce a rich result.
- **ProfilePage**: add to `schemaType` union; `Bio.tsx` emits `ProfilePage` JSON-LD + per-profile OG; default `noIndex` (matches the privacy contract).
- **Prerender/sitemap**: per-user bio pages are dynamic → not prerendered; rely on `Bio.tsx` Helmet + (optional) `/sitemap-bio.xml`. Only static explainer routes go in `PRERENDER_ROUTES` + `sitemap.xml`.

---

## Phased Delivery Order

Ship in this order — each is its own PR. Rationale: front-load the lowest-risk, highest-reuse wins that touch the fewest hot-path lines; defer the routes that modify redirect resolution and add new public surfaces.

1. **PR-1 — Branded QR (PNG color + SVG + logo) — Effort M.** Highest reuse (no new crates, no migration, no DB), non-destructive, byte-identical fallback. Ships a visible, demoable win and validates the `include_bytes!` logo-asset pattern early. Includes `ENABLE_QR_BRANDING` (default true) + `QRModal` controls + Docs/FAQ/README copy.
2. **PR-2 — Burn-after-reading — Effort S/M.** Smallest behavioral change: rides the existing `max_clicks` cache-skip + overshoot guard. One migration (`m..0020`), two struct fields, `is_active()` tweak, create/edit toggle. Flag default off. Establishes the migration + AppSettings + flag-in-five-files muscle memory for later PRs.
3. **PR-3 — Safe-link interstitial + reputation — Effort M.** Extends the **already-existing** `Preview.tsx` + `/:code/preview` surface, so the fast redirect is untouched for normal links. Internal-blocklist verdict first; external API behind `REPUTATION_API_URL` (fail-open). Flag default off.
4. **PR-4 — Smart conditional routing — Effort L.** The most invasive (modifies redirect *resolution*, adds `routing_rules` table + `has_routing_rules` to `CachedLink` + cache invalidation). Land it after the cache/flag patterns are battle-tested by PRs 2-3. Flag default off → existing links degrade to plain redirects when off.
5. **PR-5 — Link-in-Bio — Effort L.** New public unauthenticated surface, new route ordering, SEO/sitemap/robots work, privacy contract (double opt-in). Most product/marketing surface area; ship last so the supporting flag + SEO plumbing already exists. Flag default off.
6. **PR-6 (optional) — QR PDF export — Effort S/M.** The only piece needing a new crate (`printpdf`); ship after the core lands if owner wants it.

A final **PR-7 — marketing/SEO sweep** can consolidate Home/Features/Pricing/README/SEO copy if not done incrementally per feature PR (recommended: do copy in each feature's PR, reserve PR-7 for the FAQPage/`<SEO/>` wiring + featureList sync).

---

## Verification

For every PR: `rtk cargo test` (backend), `rtk cargo clippy`, `rtk vitest`/`rtk jest` (frontend), `rtk next build`/Vite build, and a live smoke against a local `docker-compose up` stack (Postgres + optional Redis).

**Per-feature end-to-end:**

- **Branded QR**: `cargo test` for the handler (plain == today, color, svg, logo-None path, flag-off). Live: `curl 'localhost:3000/links/1/qr?color=2f37d8&logo=1&format=png' -o qr.png` then open it; `?format=svg` → inspect `<svg>`; decode the PNG with a phone scanner to confirm scannability at EC-H+logo. **Screenshot**: the `QRModal` with color picker + logo toggle + the rendered branded code.
- **Burn-after-reading**: `cargo test` for `is_active()`/redirect (302 once → 410 after). Live: create a link with burn+N=1, `curl -i localhost:3000/<code>` twice (expect 302 then 410 with burn copy), confirm `burned_at` set in DB, confirm `link:<code>` is never cached. **Screenshot**: dashboard "burned" badge + the burned-link 410 page.
- **Safe-link interstitial**: `cargo test` (`preview_link` returns reputation when on, omitted when off, blocklisted → malicious, API-timeout → unknown). Live: hit `/<code>/preview` and `/<code>+`; confirm `/<code>` (safe) still 302s instantly with **no** detour (regression). **Screenshot**: the three Preview states (safe ✓ / suspicious / unknown).
- **Smart routing**: `cargo test` for `resolve_destination` (device/os/country/lang/time/weight/fallback) + handler round-trip + cache invalidation. Live: create rules (Mobile→A, default→B), `curl -A '<iPhone UA>' localhost:3000/<code>` (→A) vs `curl -A '<desktop UA>'` (→B); flip flag off → both →B; confirm routed link bypasses cache. **Screenshot**: the rule editor in `EditModal` + a terminal showing two UAs resolving to two destinations.
- **Link-in-Bio**: `cargo test` (public endpoint enabled+visible vs disabled→404 vs flag-off→404; PII absent; slug validation). Frontend `Bio.test.tsx`. Live: enable instance flag + `bio_enabled`, claim a username, mark links visible, open `/@<username>`; toggle `bio_enabled` off → page 404s; confirm `robots.txt` `Disallow: /@`. **Screenshot**: the public bio page + the Settings opt-in card.

**Aggregate gate before merge:** full `cargo test` green, `clippy` clean, `vitest`/`jest` green, Vite build succeeds, all five `ENABLE_*` flags present in `.env.example`, `backend/.env.example`, `docker-compose.yml`, and both portainer compose files (the doc-drift trap), and `/auth/settings` returns the new booleans (`curl localhost:3000/auth/settings | jq`).


---

## Appendix — Key files referenced (all absolute):**
- Backend hot path / handlers: `/Users/yevhen/work/opn.onl/backend/src/handlers/links.rs` (redirect `860-996`, `get_qr_code` `1141-1206`, `preview_link` `799-842`, `record_click_buffered` `999-1043`, request structs `322-351`)
- `/Users/yevhen/work/opn.onl/backend/src/handlers/auth.rs` (`AppSettingsResponse` `610-619`, `get_app_settings` `630-669`)
- `/Users/yevhen/work/opn.onl/backend/src/utils/cache.rs` (`CachedLink` `6-46`), `/Users/yevhen/work/opn.onl/backend/src/utils/geoip.rs` (`108-185`)
- `/Users/yevhen/work/opn.onl/backend/src/entity/links.rs` (`is_active` `100-156`), `/Users/yevhen/work/opn.onl/backend/src/entity/users.rs` (`6-29`, already has profile cols)
- `/Users/yevhen/work/opn.onl/backend/migration/src/lib.rs` (latest `m20220101_000019`; next slot `m20220101_000020`), pattern `/Users/yevhen/work/opn.onl/backend/migration/src/m20220101_000018_add_link_pinned.rs`
- Routes `/Users/yevhen/work/opn.onl/backend/src/main.rs` (`/:code` last, line `381`; `/:code/preview` `380`; QR `317`)
- Frontend: `/Users/yevhen/work/opn.onl/frontend/src/components/dashboard/QRModal.tsx`, `EditModal.tsx`; pages `Preview.tsx` (`159-175`), `Home.tsx` (`54-61`), `Features.tsx`, `Faq.tsx`, `Docs.tsx`, `Pricing.tsx`; `App.tsx` (`83-84`); `components/SEO.tsx` (`59-70`, `schemaType` `11`); `config/api.ts` (`linkQr` `44`); `vite.config.ts`, `public/{sitemap.xml,robots.txt}`, `index.html`
- Logo asset `/Users/yevhen/work/opn.onl/frontend/public/logo.png` (498×256) → embed as `/Users/yevhen/work/opn.onl/backend/assets/qr-logo.png` via `include_bytes!`
- Flag docs: `/Users/yevhen/work/opn.onl/.env.example`, `/Users/yevhen/work/opn.onl/backend/.env.example`, `/Users/yevhen/work/opn.onl/docker-compose.yml`, `/Users/yevhen/work/opn.onl/docker-compose.portainer.amd64.yml` (+ arm64), `/Users/yevhen/work/opn.onl/README.md`

**Two corrections to the exploration findings worth flagging:** the preview response struct is named `LinkPreviewResponse` (not `PreviewResponse`), and `preview_link` is **unauthenticated** — both matter for Feature 5's struct extension. Also confirmed `users::Model` already carries `display_name/bio/website/avatar_url/location`, so link-in-bio needs only `bio_username`/`bio_enabled`/`bio_theme`, not a full profile migration.
