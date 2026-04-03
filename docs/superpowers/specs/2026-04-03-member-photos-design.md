# Member Photos — Design Spec

**Issue:** #16 — User needs to set their photo to be seen on the band members menu

**Goal:** Band members can upload a profile photo from their phone. Photos appear as circular avatars on the landing page, replacing the letter initial.

---

## Upload Flow

1. Member opens Settings modal in their mixer page
2. Taps "Change Photo" → phone camera/gallery picker opens (`<input type="file" accept="image/*">`)
3. Frontend resizes selected image client-side:
   - Crop to square (center crop)
   - Resize to 128×128 px
   - Export as JPEG, quality 0.85
   - Convert to base64
4. Frontend POSTs base64 JPEG to `POST /api/members/{id}/photo`
5. Server saves to `{config_dir}/photos/{member_id}.jpg`
6. Settings modal updates to show the new photo immediately

## Storage

Photos stored as flat files: `{config_dir}/photos/{member_id}.jpg`

- Follows the `CustomizationStore` pattern (per-member files in config dir)
- No database needed
- Path traversal protection via `validate_member_id()`
- `{config_dir}` = `%APPDATA%\iem-mixer\` on the deployed Windows machine

## API Endpoints

### `POST /api/members/{id}/photo`

- **Body:** `{ "photo": "<base64 jpeg data>" }`
- **Auth:** Token required. Members can only set their own photo. Engineers can set any member's photo.
- **Response:** `200 { "ok": true }` or `400`/`403` on error
- **Behavior:** Decodes base64, validates it's a reasonable size (< 256 KB decoded), writes to `photos/{id}.jpg`

### `GET /api/members/{id}/photo`

- **Auth:** None (landing page loads before login)
- **Response:** Raw JPEG with `Content-Type: image/jpeg` and `Cache-Control: public, max-age=3600`
- **404** if no photo set for this member

### `DELETE /api/members/{id}/photo`

- **Auth:** Same as POST (own photo or engineer)
- **Response:** `200 { "ok": true }`
- **Behavior:** Deletes `photos/{id}.jpg`, member reverts to initial letter on landing page

### `GET /api/members` (existing, modified)

- Add `has_photo: bool` field to `MemberInfo` response
- Frontend uses this to decide whether to render `<img>` or letter initial
- Avoids N+1 requests to check each member's photo

## Frontend Changes

### Landing Page (`landing.rs`)

- `MemberInfo` struct gains `has_photo: bool`
- `MemberGrid` component: if `has_photo`, render `<img src="/api/members/{id}/photo" class="avatar-photo">` inside the avatar div. Otherwise, render the current letter initial.
- CSS: `img.avatar-photo` — `width: 100%; height: 100%; object-fit: cover; border-radius: 50%;`

### Settings Modal (`settings_modal.rs`)

- Add "Profile Photo" section above existing settings
- Shows current photo (circular preview) or letter initial if none
- "Change Photo" button → hidden `<input type="file" accept="image/*">` triggered by click
- On file select: read via FileReader, draw to canvas (128×128 center crop), export JPEG base64, POST to API
- "Remove Photo" button (visible only when photo exists) → DELETE to API
- Update preview immediately on success

### Client-Side Image Processing

All in the browser, no server-side image libraries:

```
FileReader.readAsDataURL(file)
  → Image.onload
  → Canvas (128×128, drawImage with center-crop)
  → canvas.toDataURL("image/jpeg", 0.85)
  → strip "data:image/jpeg;base64," prefix
  → POST to API
```

## Auth Rules

| Action | Member (own) | Member (other) | Engineer |
|--------|:---:|:---:|:---:|
| View photo | yes | yes | yes |
| Set photo | yes | no | yes |
| Delete photo | yes | no | yes |

## Fallback Behavior

- No photo set → letter initial (current behavior, unchanged)
- Photo load error → letter initial fallback via `onerror` handler on `<img>`
- API returns 404 for missing photo → frontend shows initial

## Files to Create/Modify

| File | Change |
|------|--------|
| `iem-server/src/photo_store.rs` | **New** — PhotoStore (save/load/delete JPEG files) |
| `iem-server/src/routes.rs` | Add photo endpoints, add `has_photo` to MemberInfo |
| `iem-server/src/lib.rs` | Register photo routes, add PhotoStore to AppState |
| `iem-ui/src/api.rs` | Add `has_photo` to MemberInfo, add photo API calls |
| `iem-ui/src/pages/landing.rs` | Render `<img>` when `has_photo` is true |
| `iem-ui/src/components/settings_modal.rs` | Add photo upload/remove UI section |
| `iem-ui/style.css` | Avatar photo styles |
| `e2e/tests/member-photo.spec.ts` | **New** — E2E tests for photo upload flow |
