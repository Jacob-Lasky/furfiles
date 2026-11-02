# Frontend test anchors (`data-testid`)

Durable selectors for UI probes (`/test-ux`) and component tests (#12). Prefer
these over structural selectors (`.pcard`, `.cday`, …) — markup and styling get
refactored; these anchors are a contract that survives it.

**Rule:** when a probe needs an element that has no anchor, add one here in the
same change rather than reaching for a class selector. Keep this table current.

| `data-testid` | Where | What it marks | Extra data attrs |
| --- | --- | --- | --- |
| `fab` | App shell (`SpeedDial.vue`) | the floating + — a menu button that opens the speed-dial (#77) | `aria-expanded` = `true`/`false` |
| `dial-menu` | App shell (`SpeedDial.vue`) | the speed-dial's action list (`role="menu"`); hidden (`visibility`) while closed | |
| `dial-scrim` | App shell (`SpeedDial.vue`) | the veil behind the open dial; tapping it closes the dial. Present only while open | |
| `dial-add-record` | Speed-dial, every page | "Add a record" → the add-record sheet; the action nearest the + | |
| `dial-import` | Speed-dial, every page but `/import` | "Import documents" → `/import` | |
| `dial-edit-pet` | Speed-dial, pet profile (`Pet.vue`) | "Edit <name>" → the profile editor (`pet-sheet`), archived pets included. Replaces the old `edit-profile` button (#77) | |
| `dial-add-pet` | Speed-dial, Home (`Home.vue`) | "Add a pet" → the add-pet sheet (the `add-pet-card` stays in the grid too) | |
| `dial-add-contact` | Speed-dial, Contacts (`Contacts.vue`) | "Add a contact" → a new contact sheet. Replaces the old `add-contact` page button (#77) | |
| `pet-card` | Home grid (`PetCard.vue`) | one pet's card (a link to the profile) | `data-pet` = pet id |
| `pet-card-next` | Home grid (`PetCard.vue`) | the "what's next" chip on a card (soonest reminder, or "all caught up") | |
| `pet-card-stage` | Home grid (`PetCard.vue`) | the life-stage chip (Puppy, Senior, Colt…) after the age; absent with no birth date, no cited bands, or an archived pet (#73) | |
| `add-pet-card` | Home grid (`Home.vue`) | the trailing "Add a pet" card | |
| `home-remember-card` | Home (`Home.vue`) | a pet that passed, under "Remembering" (a link to its profile) (#3) | |
| `home-past-card` | Home (`Home.vue`) | a rehomed pet, under "Past companions" (a link to its profile) (#3) | |
| `home-archived-photo` | Home (`Home.vue`) | an archived pet's portrait inside a remember/past card; only when it has a photo (otherwise the initial) | |
| `family-portrait` | Home (`Home.vue`) | the family portrait at the top of Home, when one is set (#75) | `data-source` = `uploaded` / `generated` |
| `portrait-stale` | Home (`Home.vue`) | "The family has changed since this was made" — a generated portrait whose active roster moved on | |
| `portrait-regenerate` | Home (`Home.vue`) | opens the generator (generated portraits only; tinted when stale) | |
| `portrait-replace` | Home (`Home.vue`) | opens the 3:2 crop editor to upload a photo instead ("Replace" / "Use a photo") | |
| `portrait-remove` | Home (`Home.vue`) | removes the portrait (after a confirm) | |
| `portrait-invite` | Home (`Home.vue`) | the slim "Make a family portrait" invitation when none is set | |
| `portrait-invite-generate` / `portrait-invite-upload` | Home (`Home.vue`) | the invitation's two actions | |
| `portrait-invite-dismiss` | Home (`Home.vue`) | waves the invitation off (remembered in localStorage) | |
| `portrait-later` | Home (`Home.vue`) | the quiet "Make a family portrait" link left after dismissing | |
| `portrait-sheet` | Generator (`FamilyPortraitSheet.vue`) | the generator sheet root | |
| `portrait-preview` | Generator (`FamilyPortraitSheet.vue`) | the collage canvas | |
| `portrait-names` | Generator (`FamilyPortraitSheet.vue`) | the "Show names" checkbox | |
| `portrait-missing` | Generator (`FamilyPortraitSheet.vue`) | the note naming pets without a profile photo | |
| `portrait-use-photo` / `portrait-save` | Generator (`FamilyPortraitSheet.vue`) | switch to uploading a photo / save the collage | |
| `pet-name` | Pet profile (`Pet.vue`) | the pet's name heading | |
| `next-up` | Pet profile (`Pet.vue`) | the "what's next" card in the hero (soonest reminder, or "all caught up"); absent on an archived pet | |
| `archived-banner` | Pet profile (`Pet.vue`) | replaces `next-up` on an archived pet: heading + date (#3) | `data-kind` = `passed` / `rehomed` |
| `archived-note` | Pet profile (`Pet.vue`) | the archive note inside the banner | |
| `archived-edit` | Pet profile (`Pet.vue`) | opens the archive sheet to edit kind/date/note (#37) | |
| `archived-restore` | Pet profile (`Pet.vue`) | "Bring back to active" (unarchive) | |
| `reminders-paused` | Pet profile (`Pet.vue`) | replaces the "N upcoming" count on an archived pet's reminders (#3) | |
| `weight-trend` | Pet profile (`Pet.vue`) | the weight trend chart (`<svg>`) | |
| `timeline-event` | Pet profile (`Pet.vue`) | a tappable timeline record | `data-record` = record id |
| `timeline-milestone` | Pet profile (`Pet.vue`) | a birthday / gotcha milestone row | `data-kind` = `birthday`\|`gotcha` |
| `cost-donut` | Costs (`Costs.vue`) | the spend donut (`<svg>`) | |
| `insurance-row` | Costs (`Costs.vue`) | one per-pet insurance row | `data-pet` = pet id |
| `insurance-value-card` | Costs (`Costs.vue`) | the "Insurance value" card: per pet, paid in vs paid back across its policies (#74); absent when no pet has a policy | |
| `insurance-value-row` | Costs (`Costs.vue`) | one pet in the Insurance value card (totals, bar, net, current-term hint) | `data-pet` = pet id |
| `calendar-day` | Calendar (`Calendar.vue`) | a month-grid day cell | |
| `calendar-event` | Calendar (`Calendar.vue`) | a record/reminder/milestone chip (month or agenda) | `data-kind` = `record`\|`reminder`\|`milestone` |
| `add-sheet` | Add-record modal (`AddRecordSheet.vue` via `SheetShell`) | the add-record sheet root | |
| `add-submit` | Add-record modal | the Save button | |
| `record-title-input` | Shared record form (`RecordForm.vue`) | the title input (add + edit sheets) | |
| `record-date-input` | Shared record form (`RecordForm.vue`) | the visit date (`occurred_on`) input | |
| `record-cost-input` | Shared record form (`RecordForm.vue`) | the total cost input (dollars) | |
| `add-file-input` | Add-record modal (`AddRecordSheet.vue`) | the hidden document file input (photos/PDFs; also the AI scan source) | |
| `add-extract-btn` | Add-record modal (`AddRecordSheet.vue`) | "Extract details with AI" (shown when a scannable file is attached + AI is on) | |
| `add-error` | Add-record modal (`AddRecordSheet.vue`) | the sheet's error line (a failed scan's provider message lands here, e.g. #87's 400) | |
| `record-sheet` | Record modal (`RecordSheet.vue`) | the sheet root (view + edit, #44) | |
| `record-view` | Record modal (`RecordSheet.vue`) | the read-only view body (#44); absent in edit mode | |
| `record-view-pdf` | Record modal (`RecordSheet.vue`) | an inline-rendered PDF attachment `<iframe>` in the view (#44) | |
| `record-view-atts` | Record modal (`RecordSheet.vue`) | the inline-attachments container in the view (#44) | |
| `record-edit` | Record modal (`RecordSheet.vue`) | the Edit button in the view → flips to the form (#44) | |
| `record-close` | Record modal (`RecordSheet.vue`) | the Close button in the view (#44) | |
| `record-cancel` | Record modal (`RecordSheet.vue`) | the Cancel button in edit mode → returns to the view (#44) | |
| `record-save` | Record modal (`RecordSheet.vue`) | the Save-changes button (edit mode) | |
| `record-delete` | Record modal (`RecordSheet.vue`) | the Delete button (edit mode) | |
| `reminder-sheet` | Reminder modal (`ReminderSheet.vue`) | the reminder sheet root | |
| `record-contact-select` | Shared record form (`RecordForm.vue`) | the clinic/vet picker (#20; shown only when contacts exist) | |
| `care-team` | Pet profile (`Pet.vue`) | the care-team card (#20) | |
| `care-contact` | Pet profile (`Pet.vue`) | one linked contact row (tap to edit, × to unlink) | |
| `add-care-contact` | Pet profile (`Pet.vue`) | the "New contact" button in the care team | |
| `contact-row` | Contacts page (`Contacts.vue`) | one contact row (tap to edit) | |
| `contact-sheet` | Contact modal (`ContactSheet.vue` via `SheetShell`) | the sheet root (view + add/edit, #44) | |
| `contact-view` | Contact modal (`ContactSheet.vue`) | the read-only view body (#44); existing contacts open here first | |
| `contact-edit` | Contact modal (`ContactSheet.vue`) | the Edit button in the view → flips to the form (#44) | |
| `contact-close` | Contact modal (`ContactSheet.vue`) | the Close button in the view (#44) | |
| `contact-cancel` | Contact modal (`ContactSheet.vue`) | the Cancel button (form); returns to view for an existing contact, closes for a new one (#44) | |
| `contact-name-input` | Contact modal (`ContactSheet.vue`) | the name input | |
| `contact-role-select` | Contact modal (`ContactSheet.vue`) | the role picker | |
| `contact-doctor-input` | Contact modal (`ContactSheet.vue`) | the "Doctor / contact person" input | |
| `contact-save` | Contact modal (`ContactSheet.vue`) | the Save button | |
| `contact-delete` | Contact modal (`ContactSheet.vue`) | the Delete button (edit mode only) | |
| `contact-import-url` | Contact modal (`ContactSheet.vue`) | the "import from a website" URL input (shown when AI configured) | |
| `contact-import-btn` | Contact modal (`ContactSheet.vue`) | the Fetch button for the web import | |
| `requirement-row` | Contact modal (`ContactSheet.vue`) | one editable requirement row (label + detail) | |
| `add-requirement` | Contact modal (`ContactSheet.vue`) | the "+ Add a requirement" button | |
| `hours-row` | Contact modal (`ContactSheet.vue`) | one editable hours row (label + detail) | |
| `add-hours` | Contact modal (`ContactSheet.vue`) | the "+ Add hours" button | |
| `insurance-section` | Pet profile (`Pet.vue`) | the insurance/wellness-plans card (#8/#29) | |
| `insurance-policy-row` | Pet profile (`Pet.vue`) | one policy/plan row (tap to view); distinct from Costs' `insurance-row` | `data-policy` = policy id |
| `add-insurance` | Pet profile (`Pet.vue`) | the "+ Add insurance" button | |
| `insurance-sheet` | Insurance modal (`InsuranceSheet.vue` via `SheetShell`) | the sheet root (view + add/edit, #44) | |
| `insurance-view` | Insurance modal (`InsuranceSheet.vue`) | the read-only view body; existing policies open here first | |
| `insurance-view-doc` | Insurance modal (`InsuranceSheet.vue`) | the inline document block (PDF `<iframe>` / image) in the view | |
| `insurance-value` | Insurance modal (`InsuranceValue.vue` in the view) | this term's paid in vs paid back section (#74) | |
| `insurance-value-in` / `insurance-value-back` | Insurance modal (`InsuranceValue.vue`) | the paid-in and paid-back amounts | |
| `insurance-value-bar` | Insurance modal (`InsuranceValue.vue`) | the paid back vs not-back bar (absent for a wellness plan with no payouts) | |
| `insurance-value-net` | Insurance modal (`InsuranceValue.vue`) | the net sentence ("$x more paid in than back") | |
| `insurance-value-breakeven` | Insurance modal (`InsuranceValue.vue`) | the accident & illness break-even sentence + formula | |
| `insurance-value-prev` | Insurance modal (`InsuranceValue.vue`) | the preventative allowance sentence (with "$x of Vaccines left" chips when exact) | |
| `insurance-value-missing` | Insurance modal (`InsuranceValue.vue`) | what the policy doesn't record yet ("Add the deductible…") | |
| `insurance-value-wellness` | Insurance modal (`InsuranceValue.vue`) | the wellness-plan line (paid at the counter, no claims) | |
| `insurance-value-payout` | Insurance modal (`InsuranceValue.vue`) | one visit whose payout counts toward this term | |
| `insurance-edit` | Insurance modal (`InsuranceSheet.vue`) | the Edit button in the view → flips to the form | |
| `insurance-close` | Insurance modal (`InsuranceSheet.vue`) | the Close button in the view | |
| `insurance-cancel` | Insurance modal (`InsuranceSheet.vue`) | the Cancel button (form); returns to view for an existing policy | |
| `insurance-file-input` | Insurance modal (`InsuranceSheet.vue`) | the policy-document file input (also the AI extract source) | |
| `insurance-extract-btn` | Insurance modal (`InsuranceSheet.vue`) | the "Extract details with AI" button (shown when a file is picked + AI configured) | |
| `insurance-doc-remove` | Insurance modal (`InsuranceSheet.vue`) | the Remove button for an already-attached document | |
| `insurance-provider-input` | Insurance modal (`InsuranceSheet.vue`) | the provider input | |
| `insurance-kind-select` | Insurance modal (`InsuranceSheet.vue`) | the kind picker (insurance/wellness) | |
| `insurance-status-select` | Insurance modal (`InsuranceSheet.vue`) | the status picker | |
| `insurance-reimburse-input` | Insurance modal (`InsuranceSheet.vue`) | the reimbursement % input (insurance only) | |
| `insurance-premium-input` | Insurance modal (`InsuranceSheet.vue`) | the premium input | |
| `coverage-row` | Insurance modal (`InsuranceSheet.vue`) | one editable coverage row (label + detail) | |
| `add-coverage` | Insurance modal (`InsuranceSheet.vue`) | the "+ Add coverage" button | |
| `exclusion-row` | Insurance modal (`InsuranceSheet.vue`) | one editable exclusion row (label + detail) | |
| `add-exclusion` | Insurance modal (`InsuranceSheet.vue`) | the "+ Add exclusion" button | |
| `insurance-also-pet` | Insurance modal (`InsuranceSheet.vue`) | the "show on profile as insurer" checkbox | |
| `insurance-save` | Insurance modal (`InsuranceSheet.vue`) | the Save button | |
| `insurance-delete` | Insurance modal (`InsuranceSheet.vue`) | the Delete button (edit mode only) | |
| `pet-color-tag` | Pet profile (`Pet.vue`) | the coat-colour tag in the hero (#63) | |
| `pet-sex-tag` | Pet profile (`Pet.vue`) | the sex tag in the hero, worded for the species (Gelding, Spayed female, Bitch); absent when not recorded or `unknown` (#72) | |
| `pet-life-stage` | Pet profile (`Pet.vue`) | the life-stage tag in the hero; absent when it would repeat the sex tag (an adult horse) (#73) | |
| `sex-suggestion` | Pet profile (`SexSuggestionChip.vue`) | "Update to Spayed female?" — a spay/neuter record while the sex is intact (#72) | |
| `sex-suggestion-accept` | Pet profile (`SexSuggestionChip.vue`) | one tap: PATCHes the pet's sex to the suggested value | |
| `sex-suggestion-record` | Pet profile (`SexSuggestionChip.vue`) | opens the record the suggestion came from | |
| `display-settings` | Settings (`Settings.vue`) | the Display card (#72) | |
| `breeder-terms-toggle` | Settings (`Settings.vue`) | "Use breeder terms" checkbox; saves on change | |
| `allergy-alert` | Pet profile (`Pet.vue`) | the medical-alert strip listing known allergies (#63); absent when none | |
| `care-section` | Pet profile (`Pet.vue`) | the ongoing-care card (food/preventatives/meds/supplements, #63) | |
| `care-regimen` | Pet profile (`Pet.vue`) | one CURRENT care row (tap to view) | `data-kind` = care kind |
| `care-regimen-past` | Pet profile (`Pet.vue`) | one stopped care row (demoted below the current ones) | |
| `add-regimen` | Pet profile (`Pet.vue`) | the "+ Add food, medication or preventative" button | |
| `pet-sheet` | Profile modal (`PetSheet.vue` via `SheetShell`) | the sheet root; edit-only (the profile page IS the view) | |
| `pet-archive-open` | Profile modal (`PetSheet.vue`) | "Has <name> passed away or moved on?" hand-off to the archive sheet; active pets only (#3) | |
| `archive-sheet` | Archive modal (`ArchiveSheet.vue` via `SheetShell`) | the sheet root: archive a pet, or edit an existing archive (#3) | |
| `archive-kind-passed` / `archive-kind-rehomed` | Archive modal | the two "what happened" option cards (each wraps a radio) | |
| `archive-date` | Archive modal | date of passing / date they moved on | |
| `archive-note` | Archive modal | memorial line / where they went | |
| `archive-save` / `archive-cancel` | Archive modal | save / close | |
| `pet-name-input` | Profile modal (`PetSheet.vue`) | the name input | |
| `pet-species-select` | Profile modal (`PetSheet.vue`) | the species picker (keeps a non-preset species) | |
| `pet-breed-input` | Profile modal (`PetSheet.vue`) | the breed input | |
| `pet-color-input` | Profile modal (`PetSheet.vue`) | the coat-colour input | |
| `pet-sex-input` | Profile modal (`PetSheet.vue`) | the sex `<select>` (`""` = not recorded, else one of `SEXES`, #72) | |
| `pet-birth-input` | Profile modal (`PetSheet.vue`) | the birthday date input | |
| `pet-gotcha-input` | Profile modal (`PetSheet.vue`) | the gotcha-day date input | |
| `pet-microchip-input` | Profile modal (`PetSheet.vue`) | the microchip-number input | |
| `allergy-row` | Profile modal (`PetSheet.vue`) | one editable allergy row (allergen + reaction) | |
| `add-allergy` | Profile modal (`PetSheet.vue`) | the "+ Add an allergy" button | |
| `pet-cancel` | Profile modal (`PetSheet.vue`) | the Cancel button (closes; there is no view to return to) | |
| `pet-save` | Profile modal (`PetSheet.vue`) | the Save button | |
| `regimen-sheet` | Care modal (`RegimenSheet.vue` via `SheetShell`) | the sheet root (view + add/edit, #44) | |
| `regimen-view` | Care modal (`RegimenSheet.vue`) | the read-only view body; existing entries open here first | |
| `regimen-edit` | Care modal (`RegimenSheet.vue`) | the Edit button in the view → flips to the form | |
| `regimen-close` | Care modal (`RegimenSheet.vue`) | the Close button in the view | |
| `regimen-cancel` | Care modal (`RegimenSheet.vue`) | the Cancel button (form); returns to view for an existing entry | |
| `regimen-kind-pick` | Care modal (`RegimenSheet.vue`) | the kind chip row (food/preventative/medication/supplement) | buttons carry `data-kind` |
| `regimen-product-input` | Care modal (`RegimenSheet.vue`) | the food/product name input | |
| `regimen-amount-input` | Care modal (`RegimenSheet.vue`) | the amount/dose input | |
| `regimen-frequency-input` | Care modal (`RegimenSheet.vue`) | the "how often" input | |
| `regimen-protects` | Care modal (`RegimenSheet.vue`) | the protection toggles; PREVENTATIVE ONLY | buttons carry `data-protection` |
| `regimen-still-on` | Care modal (`RegimenSheet.vue`) | the "currently on this" checkbox (drives `ended_on`) | |
| `regimen-ended-input` | Care modal (`RegimenSheet.vue`) | the stopped-date input; only shown when NOT current | |
| `regimen-save` | Care modal (`RegimenSheet.vue`) | the Save button | |
| `regimen-delete` | Care modal (`RegimenSheet.vue`) | the Remove button (edit mode only) | |
| `add-pet-sheet` | Add-pet modal (`AddPetSheet.vue` via `SheetShell`) | the sheet root | |
| `add-pet-name` | Add-pet modal (`AddPetSheet.vue`) | the name input | |
| `add-pet-species` | Add-pet modal (`AddPetSheet.vue`) | the species picker | |
| `add-pet-breed` | Add-pet modal (`AddPetSheet.vue`) | the breed input | |
| `add-pet-sex` | Add-pet modal (`AddPetSheet.vue`) | the sex `<select>` (`""` = not recorded, else one of `SEXES`, #72) | |
| `add-pet-birth` | Add-pet modal (`AddPetSheet.vue`) | the born date input | |
| `add-pet-weight` | Add-pet modal (`AddPetSheet.vue`) | the weight input | |
| `add-pet-unit` | Add-pet modal (`AddPetSheet.vue`) | the weight-unit picker | |
| `add-pet-cancel` | Add-pet modal (`AddPetSheet.vue`) | the Cancel button | |
| `add-pet-save` | Add-pet modal (`AddPetSheet.vue`) | the "Add pet" button | |
| `nav-import` | Nav rail (`App.vue`) | the Import nav item → `/import` | |
| `add-import-link` | Add-record modal (`AddRecordSheet.vue`) | the "import them all at once" link → `/import`, closes the sheet | |
| `import-page` | Import page (`Import.vue`) | the page root | |
| `import-setup` | Import page (`Import.vue`) | the "AI isn't set up" card; replaces the drop zone | |
| `import-drop` | Import page (`Import.vue`) | the drop zone (a `<label>`; gains `.over` while dragging) | |
| `import-input` | Import page (`Import.vue`) | the hidden multi-file input inside the drop zone | `setInputFiles` target |
| `import-save-all` | Import page (`Import.vue`) | "Save all N ready"; only when 2+ files are saveable | |
| `import-file` | Import card (`ImportFileCard.vue`) | one dropped file's card | `data-status` = queued / converting / analyzing / ready / saving / saved / error / unsupported |
| `import-converting` | Import card | "Converting the iPhone photo…" line (HEIC being turned into a JPEG in the browser; `data-status="converting"`) | the card then shows the `<stem>.jpg` name |
| `import-remove` | Import card | the × button | |
| `import-error` / `import-retry` | Import card | the error text / "Try again" (error state only, and only when the file was read: a failed HEIC conversion offers none) | |
| `import-saved` | Import card | the "Saved …" summary + per-pet links | |
| `import-file-dup` | Import card | "Already in FurFiles" notice when the exact file is attached somewhere | |
| `import-clinic` / `import-clinic-new` | Import card | the clinic line / the "add to Contacts" checkbox for a new clinic | |
| `import-record` | Import card | one proposed record | contains `import-record-toggle`, `import-record-pet`, `import-record-dup` |
| `import-clinic-link` | Import card | "mark this saved visit as at the clinic" row (a duplicate visit with no clinic) | |
| `import-careteam` / `import-weight` / `import-reminder` / `import-done` / `import-petupdate` / `import-regimen` | Import card | one "Also update" row of each kind | |
| `import-claim` / `import-claim-pet` / `import-policy` / `import-policy-pet` / `import-policy-end` | Import card | one insurance row of each kind; the `-pet` selects re-assign a claim or policy to another pet (a re-assigned claim drops its matched visit) | |
| `import-claim-visit` | Import card | the visit picker inside a claim row; only when the claim matched more than one saved visit that day | |
| `import-need-pet` | Import card | "Pick a pet for N records first" (blocks save) | |
| `import-save` | Import card | the per-file save button; its label says what it writes | |
| `settings-provider` | Settings (`Settings.vue`) | the provider, as read-only text (Anthropic is the only one implemented) (#67) | |
| `settings-api-key` | Settings | the API-key input; a pasted key fills the model picker before saving (#67) | |
| `settings-key-error` | Settings | why a pasted key was refused (another vendor's key, or rejected by Anthropic) (#67) | |
| `settings-model-select` | Settings | the model picker: the key's models newest first, then "Custom…"; absent with no list (#67) | option value `__custom__` = Custom |
| `settings-model-custom` | Settings | the free-text model input: shown for "Custom…", or alone when there is no list (#67) | |
| `settings-model-note` | Settings | the line under the model: loading, the list's error, or "paste your API key" (#67) | |

`SheetShell.vue` takes a `testid` prop that stamps the sheet root, so every
bottom-sheet gets its anchor the same way.
