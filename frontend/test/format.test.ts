import { describe, it, expect } from 'vitest'
import { money, dueLabel, isOverdue, ageFrom, petGradient, smoothPath, weightTrend, categoryColor, monthYear, todayISO, CATEGORIES, fileSize, isImage, isPdf, readDataUrl, renderMarkdown, speciesTrack, centsFromDollars, dollarsFromCents, itemsToLineItems, lineItemsToItems, rowsToExtra, extraToRows, petMilestones, nextAnniversary, monthMilestones, upcomingMilestones, milestoneIcon, contactRoleLabel, labeledToRows, rowsToLabeled, mergeLabeledRows, insuranceKindLabel, premiumIntervalSuffix, regimenKindLabel, regimenKindIcon, protectionLabel, regimenDose, isCurrentCare, currentProtection, longDate, isRehomed, archiveHeading, sexLabel, sexOptions, lifeStage, petLifeStage, sexAndStageTags,} from '../src/format'

describe('fileSize', () => {
  it('formats bytes, KB, and MB', () => {
    expect(fileSize(48)).toBe('48 B')
    expect(fileSize(2048)).toBe('2 KB')
    expect(fileSize(5 * 1024 * 1024)).toBe('5.0 MB')
    expect(fileSize(null)).toBe('')
  })
})

describe('isImage', () => {
  it('is true only for image mime types', () => {
    expect(isImage('image/png')).toBe(true)
    expect(isImage('application/pdf')).toBe(false)
    expect(isImage(null)).toBe(false)
  })
})

describe('isPdf', () => {
  it('is true only for the pdf mime type', () => {
    expect(isPdf('application/pdf')).toBe(true)
    expect(isPdf('image/png')).toBe(false)
    expect(isPdf(null)).toBe(false)
    expect(isPdf(undefined)).toBe(false)
  })
})

describe('readDataUrl', () => {
  it('reads a blob as a base64 data URL', async () => {
    const url = await readDataUrl(new File([new Uint8Array([104, 105])], 'x.png', { type: 'image/png' }))
    expect(url).toMatch(/^data:image\/png;base64,/)
    expect(url.split(',')[1]).toBe('aGk=') // "hi" → base64
  })
})

describe('renderMarkdown', () => {
  it('renders a safe subset', () => {
    expect(renderMarkdown('**bold** and *italic*')).toContain('<strong>bold</strong>')
    expect(renderMarkdown('**bold** and *italic*')).toContain('<em>italic</em>')
    expect(renderMarkdown('- one\n- two')).toBe('<ul><li>one</li><li>two</li></ul>')
    expect(renderMarkdown('1. one\n2. two\n\nafter')).toBe('<ol><li>one</li><li>two</li></ol><p>after</p>')
    expect(renderMarkdown('# Title')).toBe('<h3>Title</h3>')
    expect(renderMarkdown('[docs](https://x.com)')).toContain('<a href="https://x.com"')
    expect(renderMarkdown('')).toBe('')
  })
  it('escapes HTML and refuses non-http links (XSS guard)', () => {
    const h = renderMarkdown('<img src=x onerror=alert(1)> hi')
    expect(h).not.toContain('<img')
    expect(h).toContain('&lt;img')
    // a javascript: URI must NOT become an anchor
    const j = renderMarkdown('[click](javascript:alert(1))')
    expect(j).not.toContain('<a ')
    expect(j).toContain('[click]')
  })
})

describe('CATEGORIES', () => {
  it('is the ordered category list, derived from the color map', () => {
    // Single-source guard: CATEGORIES is Object.keys(CATEGORY_COLORS), so the
    // record picker can never drift from the colors used by the timeline,
    // calendar, and cost donut. Reordering/removing a color trips this test.
    expect(CATEGORIES).toEqual(['Appointments', 'Prescriptions', 'Food', 'Boarding', 'Grooming'])
    expect(CATEGORIES[0]).toBe('Appointments') // the record-form default
  })
})

describe('todayISO', () => {
  it('formats a date as zero-padded YYYY-MM-DD', () => {
    expect(todayISO(new Date('2026-03-05T09:30:00'))).toBe('2026-03-05')
    expect(todayISO(new Date('2026-12-25T00:00:00'))).toBe('2026-12-25')
  })
})

describe('money', () => {
  it('formats whole and fractional dollars', () => {
    expect(money(0)).toBe('$0')
    expect(money(42000)).toBe('$420')
    expect(money(9599)).toBe('$95.99')
    expect(money(null)).toBe('—')
  })
})

describe('dueLabel / isOverdue', () => {
  const today = '2026-07-12'
  it('phrases upcoming due dates neutrally, by parts (no tz drift)', () => {
    expect(dueLabel('2026-07-12', today)).toBe('due today')
    expect(dueLabel('2026-07-13', today)).toBe('due tomorrow')
    expect(dueLabel('2026-07-15', today)).toBe('due in 3 days')
    expect(dueLabel('2026-08-02', today)).toBe('due in 3 wks')
    expect(dueLabel('2026-07-19', today)).toBe('due in 1 wk')
    expect(dueLabel('2026-10-12', today)).toBe('due in 3 mo')
    expect(dueLabel('2028-07-12', today)).toBe('due in 2 yrs')
    expect(dueLabel('2027-07-12', today)).toBe('due in 1 yr')
  })
  it('marks a past due date overdue', () => {
    expect(dueLabel('2026-07-11', today)).toBe('overdue')
    expect(isOverdue('2026-07-11', today)).toBe(true)
    expect(isOverdue('2026-07-12', today)).toBe(false)
    expect(isOverdue('2026-08-01', today)).toBe(false)
  })
  it('is null-safe', () => {
    expect(dueLabel(null)).toBe('')
    expect(dueLabel(undefined)).toBe('')
    expect(isOverdue(null)).toBe(false)
  })
})

describe('ageFrom', () => {
  it('reads in months under a year, half years under two, whole years after', () => {
    const now = new Date(2026, 9, 11) // Oct 11 2026, local
    expect(ageFrom('2022-03-14', now)).toBe('4 yr')
    expect(ageFrom('2025-11-04', now)).toBe('11 mo') // Lulu: was "<1 yr"
    expect(ageFrom('2024-05-06', now)).toBe('2 yr') // Pedro
    expect(ageFrom('2025-04-11', now)).toBe('1½ yr') // 18 months on the day
    expect(ageFrom('2025-04-12', now)).toBe('1 yr') // a day short of 18 months
    expect(ageFrom('2025-10-11', now)).toBe('1 yr')
    expect(ageFrom('2026-09-20', now)).toBe('<1 mo')
    expect(ageFrom('2026-10-11', now)).toBe('<1 mo')
    expect(ageFrom('2026-12-01', now)).toBe(null) // not born yet
    expect(ageFrom(null, now)).toBe(null)
    expect(ageFrom('not-a-date', now)).toBe(null)
  })
  it('does not tick a birthday a day late west of UTC', () => {
    // Local midnight on the birthday: still exactly one year.
    expect(ageFrom('2025-10-11', new Date(2026, 9, 11, 0, 0, 1))).toBe('1 yr')
  })
})

describe('monthYear', () => {
  it('formats an ISO date as month + year, null-safe', () => {
    expect(monthYear('2024-08-30')).toBe('Aug 2024')
    expect(monthYear('2026-01-03')).toBe('Jan 2026')
    expect(monthYear(null)).toBe(null)
    expect(monthYear('not-a-date')).toBe(null)
  })
})

describe('petGradient', () => {
  it('builds a gradient with fallback', () => {
    expect(petGradient({ gradient_a: '#111', gradient_b: '#222' })).toContain('#111')
    expect(petGradient({})).toContain('#F2624C')
    expect(petGradient(null)).toContain('linear-gradient')
  })
})

describe('categoryColor', () => {
  it('maps known categories and falls back deterministically', () => {
    expect(categoryColor('Appointments')).toBe('var(--brand)')
    expect(categoryColor('Prescriptions')).toBe('#F3B44C')
    const a = categoryColor('Mystery', 0)
    const b = categoryColor('Mystery', 0)
    expect(a).toBe(b) // deterministic for same index
    expect(typeof categoryColor('Other', 3)).toBe('string')
  })
})

describe('speciesTrack', () => {
  it('maps the five design-system species to their track', () => {
    expect(speciesTrack('Dog')).toBe('dog')
    expect(speciesTrack('cat')).toBe('cat')
    expect(speciesTrack('Horse')).toBe('horse')
    expect(speciesTrack('Bird')).toBe('bird')
    expect(speciesTrack('Fish')).toBe('fish')
  })
  it('matches loosely on breed-ish / synonym free text', () => {
    expect(speciesTrack('Golden Retriever dog')).toBe('dog')
    expect(speciesTrack('Kitten')).toBe('cat')
    expect(speciesTrack('Gelding')).toBe('horse')
    expect(speciesTrack('Cockatiel')).toBe('bird')
    expect(speciesTrack('Betta')).toBe('fish')
  })
  it('resolves *fish species to fish, not the paw or a mammal', () => {
    expect(speciesTrack('Catfish')).toBe('fish')
    expect(speciesTrack('dogfish')).toBe('fish')
  })
  it('falls back to paw for unknown, empty, or missing species', () => {
    expect(speciesTrack('Rabbit')).toBe('paw')
    expect(speciesTrack('Reptile')).toBe('paw')
    expect(speciesTrack('')).toBe('paw')
    expect(speciesTrack(null)).toBe('paw')
    expect(speciesTrack(undefined)).toBe('paw')
  })
})

describe('record-form <-> API conversions (#15)', () => {
  it('centsFromDollars parses money, null on blank/NaN', () => {
    expect(centsFromDollars('185.50')).toBe(18550)
    expect(centsFromDollars('50')).toBe(5000)
    expect(centsFromDollars('')).toBe(null)
    expect(centsFromDollars(null)).toBe(null)
    expect(centsFromDollars('abc')).toBe(null)
  })
  it('dollarsFromCents is the inverse, empty for null', () => {
    expect(dollarsFromCents(18550)).toBe('185.5')
    expect(dollarsFromCents(5000)).toBe('50')
    expect(dollarsFromCents(0)).toBe('0')
    expect(dollarsFromCents(null)).toBe('')
  })
  it('itemsToLineItems drops blank descriptions and converts amounts', () => {
    expect(itemsToLineItems([
      { desc: 'Exam', amount: '50' },
      { desc: '  ', amount: '99' }, // dropped
      { desc: 'Meds', amount: '' }, // kept, null amount
    ])).toEqual([
      { description: 'Exam', amount_cents: 5000 },
      { description: 'Meds', amount_cents: null },
    ])
    expect(itemsToLineItems(null)).toEqual([])
  })
  it('lineItemsToItems is the round-trip inverse for editing', () => {
    expect(lineItemsToItems([{ description: 'Exam', amount_cents: 5000 }, { description: 'X', amount_cents: null }]))
      .toEqual([{ desc: 'Exam', amount: '50' }, { desc: 'X', amount: '' }])
    expect(lineItemsToItems(null)).toEqual([])
  })
  it('rowsToExtra keeps only complete rows, undefined when empty', () => {
    expect(rowsToExtra([{ key: 'Tag', val: '123' }, { key: '', val: 'x' }, { key: 'y', val: '' }]))
      .toEqual({ Tag: '123' })
    expect(rowsToExtra([{ key: '', val: '' }])).toBe(undefined)
    expect(rowsToExtra(null)).toBe(undefined)
  })
  it('extraToRows expands the stored object back to editable rows', () => {
    expect(extraToRows({ Tag: '123', Dose: '2ml' })).toEqual([{ key: 'Tag', val: '123' }, { key: 'Dose', val: '2ml' }])
    expect(extraToRows(null)).toEqual([])
  })
})

describe('pet milestones (#32)', () => {
  it('petMilestones derives born + came home from the pet, skipping unknown dates', () => {
    expect(petMilestones({ name: 'Pedro', birth_date: '2020-05-01', gotcha_date: '2020-08-15' })).toEqual([
      { date: '2020-05-01', subkind: 'birthday', title: 'Pedro was born' },
      { date: '2020-08-15', subkind: 'gotcha', title: 'Pedro came home' },
    ])
    expect(petMilestones({ name: 'X' })).toEqual([])
    expect(petMilestones(null)).toEqual([])
  })

  it('nextAnniversary finds the next yearly recurrence, clamping Feb 29', () => {
    expect(nextAnniversary('2020-05-01', '2026-01-10')).toBe('2026-05-01') // later this year
    expect(nextAnniversary('2020-05-01', '2026-06-10')).toBe('2027-05-01') // already passed → next year
    expect(nextAnniversary('2020-05-01', '2026-05-01')).toBe('2026-05-01') // today counts
    expect(nextAnniversary('2020-02-29', '2027-01-01')).toBe('2027-02-28') // 2027 not a leap year
    expect(nextAnniversary(null)).toBe(null)
  })

  it('monthMilestones lands anniversaries in a month, active pets only', () => {
    const pets = [
      { id: 'pedro', name: 'Pedro', birth_date: '2020-05-01', gotcha_date: '2020-08-15', archived_at: null },
      { id: 'rocco', name: 'Rocco', birth_date: '2015-05-20', archived_at: '2026-10-20' }, // memorialized → excluded
    ]
    expect(monthMilestones(pets, 2026, 4)).toEqual([ // month index 4 = May
      { day: 1, petId: 'pedro', petName: 'Pedro', subkind: 'birthday', title: 'Pedro’s birthday' },
    ])
    expect(monthMilestones(pets, 2026, 7).map((m) => m.subkind)).toEqual(['gotcha']) // August = Pedro's gotcha
    // Feb 29 birthday clamps to the 28th in a non-leap year
    expect(monthMilestones([{ id: 'x', name: 'X', birth_date: '2020-02-29', archived_at: null }], 2027, 1)[0].day).toBe(28)
  })

  it('milestoneIcon maps the subkind to a glyph', () => {
    expect(milestoneIcon('birthday')).toBe('🎂')
    expect(milestoneIcon('gotcha')).toBe('🏡')
  })

  it('upcomingMilestones gives the next occurrence per active pet', () => {
    const pets = [
      { id: 'pedro', name: 'Pedro', birth_date: '2020-05-01', gotcha_date: '2020-08-15', archived_at: null },
      { id: 'rocco', name: 'Rocco', birth_date: '2015-05-20', archived_at: '2026-10-20' }, // memorialized → excluded
    ]
    expect(upcomingMilestones(pets, '2026-06-01')).toEqual([
      { subkind: 'birthday', date: '2027-05-01', petId: 'pedro', petName: 'Pedro', title: 'Pedro’s birthday' }, // May passed → next year
      { subkind: 'gotcha', date: '2026-08-15', petId: 'pedro', petName: 'Pedro', title: 'Pedro’s gotcha day' },
    ])
  })
})

describe('smoothPath / weightTrend', () => {
  it('returns empty for too few points', () => {
    expect(smoothPath([])).toBe('')
    expect(smoothPath([[0, 0]])).toBe('')
    expect(weightTrend([])).toBe(null)
  })
  it('builds a path and endpoint for real series', () => {
    const w = weightTrend([{ value: 1 }, { value: 2 }, { value: 3 }], 300, 120)
    expect(w.line.startsWith('M ')).toBe(true)
    expect(w.line).toContain('C')
    expect(w.last).toHaveLength(2)
    expect(w.w).toBe(300)
  })
})

describe('contactRoleLabel', () => {
  it('labels every known role and falls back gracefully', () => {
    expect(contactRoleLabel('vet')).toBe('Vet')
    expect(contactRoleLabel('emergency')).toBe('Emergency vet')
    expect(contactRoleLabel('specialist')).toBe('Specialist')
    expect(contactRoleLabel('groomer')).toBe('Groomer')
    expect(contactRoleLabel('boarding')).toBe('Daycare & boarding')
    expect(contactRoleLabel('other')).toBe('Other')
    expect(contactRoleLabel('wizard')).toBe('Contact')
    expect(contactRoleLabel(null)).toBe('Contact')
    expect(contactRoleLabel(undefined)).toBe('Contact')
  })
})

describe('labeled rows (shared by contacts + insurance)', () => {
  it('round-trips LabeledDetail ↔ rows, dropping blank labels and empty details', () => {
    const rows = labeledToRows([{ label: 'Wellness exam', detail: 'Up to $70/yr' }, { label: 'Vaccines', detail: null }])
    expect(rows).toEqual([
      { label: 'Wellness exam', detail: 'Up to $70/yr' },
      { label: 'Vaccines', detail: '' },
    ])
    expect(labeledToRows(null)).toEqual([])
    expect(rowsToLabeled([
      { label: '  Surgery ', detail: '  covered ' },
      { label: '  ', detail: 'x' }, // blank label dropped
      { label: 'Exam', detail: '' }, // empty detail → null
    ])).toEqual([
      { label: 'Surgery', detail: 'covered' },
      { label: 'Exam', detail: null },
    ])
  })

  it('merges only rows whose label is not already present (case-insensitive)', () => {
    const target = [{ label: 'Exam', detail: 'yearly' }]
    mergeLabeledRows(target, [{ label: 'exam', detail: 'dup' }, { label: 'Dental', detail: null }, { label: '  ', detail: 'x' }])
    expect(target).toEqual([
      { label: 'Exam', detail: 'yearly' }, // kept, dup skipped
      { label: 'Dental', detail: '' },
    ])
  })
})

describe('insuranceKindLabel + premiumIntervalSuffix', () => {
  it('labels the kind', () => {
    expect(insuranceKindLabel('insurance')).toBe('Insurance')
    expect(insuranceKindLabel('wellness')).toBe('Wellness plan')
    expect(insuranceKindLabel(null)).toBe('Insurance')
  })
  it('suffixes the premium interval', () => {
    expect(premiumIntervalSuffix('year')).toBe('/yr')
    expect(premiumIntervalSuffix('month')).toBe('/mo')
    expect(premiumIntervalSuffix(null)).toBe('')
  })
})

// ---------- ongoing care (#63) ----------

describe('ongoing care helpers', () => {
  it('labels and ices each care kind, passing unknowns through', () => {
    expect(regimenKindLabel('food')).toBe('Food')
    expect(regimenKindLabel('preventative')).toBe('Preventative')
    expect(regimenKindLabel('medication')).toBe('Medication')
    expect(regimenKindLabel('supplement')).toBe('Supplement')
    // A kind the server grows still renders something rather than vanishing.
    expect(regimenKindLabel('dewormer')).toBe('dewormer')
    expect(regimenKindLabel(null)).toBe('Care')
    expect(regimenKindIcon('food')).toBe('🍽')
    expect(regimenKindIcon('preventative')).toBe('🛡')
    expect(regimenKindIcon('supplement')).toBe('🌿')
    expect(regimenKindIcon('medication')).toBe('💊')
    expect(regimenKindIcon(null)).toBe('💊')
  })

  it('labels a protection for display', () => {
    expect(protectionLabel('flea')).toBe('Flea')
    expect(protectionLabel('heartworm')).toBe('Heartworm')
    expect(protectionLabel('worm')).toBe('Worms')
    expect(protectionLabel('')).toBe('')
  })

  it('joins amount and frequency, skipping what is missing', () => {
    expect(regimenDose({ amount: '1 cup', frequency: '2x/day' })).toBe('1 cup · 2x/day')
    expect(regimenDose({ amount: '1 pill', frequency: null })).toBe('1 pill')
    expect(regimenDose({ amount: null, frequency: 'monthly' })).toBe('monthly')
    expect(regimenDose({ amount: null, frequency: null })).toBe('')
    expect(regimenDose({ amount: '  ', frequency: null })).toBe('')
  })

  it('treats an empty ended_on as "still on it"', () => {
    expect(isCurrentCare({ ended_on: null })).toBe(true)
    expect(isCurrentCare({ ended_on: '' })).toBe(true)
    expect(isCurrentCare({ ended_on: '2026-02-01' })).toBe(false)
  })

  it('answers "is the pet current on heartworm prevention, and what brand"', () => {
    const regimens = [
      { product: 'Simparica Trio', ended_on: null, protects: ['flea', 'tick', 'heartworm'] },
      { product: 'Heartgard', ended_on: '2026-01-01', protects: ['heartworm'] }, // stopped
      { product: "Hill's Puppy", ended_on: null, protects: null },               // food
    ]
    const hw = currentProtection(regimens, 'heartworm')
    expect(hw).toHaveLength(1)
    expect(hw[0].product).toBe('Simparica Trio')
    // One product answers all three of the intake form's prevention questions.
    expect(currentProtection(regimens, 'flea')[0].product).toBe('Simparica Trio')
    expect(currentProtection(regimens, 'tick')[0].product).toBe('Simparica Trio')
    expect(currentProtection(regimens, 'worm')).toEqual([])
    expect(currentProtection(null, 'flea')).toEqual([])
  })
})

describe('archive helpers (#3)', () => {
  it('formats a long date by parts, rejecting junk and impossible days', () => {
    expect(longDate('2026-08-01')).toBe('Aug 1, 2026')
    expect(longDate('2026-08-01 12:00:00')).toBe('Aug 1, 2026') // legacy timestamp
    expect(longDate('2026-02-31')).toBe(null)
    expect(longDate('nope')).toBe(null)
    expect(longDate(null)).toBe(null)
  })

  it('treats only an archived pet with kind rehomed as rehomed', () => {
    expect(isRehomed({ archived_at: '2026-08-01', archive_kind: 'rehomed' })).toBe(true)
    expect(isRehomed({ archived_at: '2026-08-01', archive_kind: 'passed' })).toBe(false)
    expect(isRehomed({ archived_at: '2026-08-01', archive_kind: null })).toBe(false) // legacy memorial
    expect(isRehomed({ archived_at: null, archive_kind: 'rehomed' })).toBe(false)
    expect(isRehomed(null)).toBe(false)
  })

  it('never gives a rehomed pet memorial wording', () => {
    expect(archiveHeading({ archived_at: '2026-08-01', archive_kind: 'rehomed' })).toBe('Moved on')
    expect(archiveHeading({ archived_at: '2026-10-20', archive_kind: 'passed' })).toBe('In loving memory')
  })
})

describe('sexLabel (#72)', () => {
  it('words each stored value for the species', () => {
    expect(sexLabel('male_neutered', 'Dog')).toBe('Neutered male')
    expect(sexLabel('female_spayed', 'Cat')).toBe('Spayed female')
    expect(sexLabel('male', 'Rabbit')).toBe('Male')
    expect(sexLabel('female', 'Bird')).toBe('Female')
    expect(sexLabel('unknown', 'Dog')).toBe('Unknown')
    // Horses get horse words automatically, by any horse-ish species name.
    expect(sexLabel('male', 'Horse')).toBe('Stallion')
    expect(sexLabel('female', 'Horse')).toBe('Mare')
    expect(sexLabel('male_neutered', 'Pony')).toBe('Gelding')
    expect(sexLabel('female_spayed', 'horse')).toBe('Spayed mare')
  })
  it('switches intact dogs and cats to breeder terms only when asked', () => {
    expect(sexLabel('male', 'Dog', true)).toBe('Dog')
    expect(sexLabel('female', 'Dog', true)).toBe('Bitch')
    expect(sexLabel('male', 'Cat', true)).toBe('Tom')
    expect(sexLabel('female', 'Cat', true)).toBe('Queen')
    expect(sexLabel('male_neutered', 'Dog', true)).toBe('Neutered male') // only intact values change
    expect(sexLabel('male', 'Rabbit', true)).toBe('Male')
    expect(sexLabel('male', 'Horse', true)).toBe('Stallion')
  })
  it('is blank when not recorded and shows a legacy string as typed', () => {
    expect(sexLabel(null, 'Dog')).toBe('')
    expect(sexLabel('', 'Dog')).toBe('')
    expect(sexLabel('Male · neutered', 'Dog')).toBe('Male · neutered')
  })
  it('offers all five values as picker options', () => {
    expect(sexOptions('Horse').map((o) => o.label)).toEqual(['Stallion', 'Mare', 'Gelding', 'Spayed mare', 'Unknown'])
    expect(sexOptions('Dog', true)[1]).toEqual({ value: 'female', label: 'Bitch' })
  })
})

describe('lifeStage (#73)', () => {
  const now = new Date(2026, 9, 11) // 2026-10-11, local time
  it('dogs follow the AAHA 2019 bands (upper end, 12-year lifespan)', () => {
    expect(lifeStage('Dog', '2026-02-01', null, now)).toBe('Puppy') // 8 mo
    expect(lifeStage('Dog', '2025-11-04', 'female', now)).toBe('Young adult') // Lulu, 11 mo
    expect(lifeStage('Dog', '2022-10-12', null, now)).toBe('Young adult') // a day short of 4
    expect(lifeStage('Dog', '2022-10-11', null, now)).toBe('Mature adult')
    expect(lifeStage('Dog', '2017-10-12', null, now)).toBe('Mature adult')
    expect(lifeStage('Dog', '2017-10-11', null, now)).toBe('Senior') // 9 y
  })
  it('cats follow the 2021 AAHA/AAFP bands', () => {
    expect(lifeStage('Cat', '2026-01-01', null, now)).toBe('Kitten')
    expect(lifeStage('Cat', '2025-10-11', null, now)).toBe('Young adult') // 1 y
    expect(lifeStage('Cat', '2019-10-12', null, now)).toBe('Young adult') // 6 y 11 mo
    expect(lifeStage('Cat', '2019-10-11', null, now)).toBe('Mature adult') // 7 y
    expect(lifeStage('Cat', '2017-04-18', 'male_neutered', now)).toBe('Mature adult') // Kipper, 9 y
    expect(lifeStage('Cat', '2016-10-11', null, now)).toBe('Senior') // 10 y
  })
  it('horses combine age and sex', () => {
    expect(lifeStage('Horse', '2026-03-01', 'male', now)).toBe('Foal')
    expect(lifeStage('Horse', '2025-03-01', 'female', now)).toBe('Yearling')
    expect(lifeStage('Horse', '2023-03-01', 'male', now)).toBe('Colt')
    expect(lifeStage('Horse', '2023-03-01', 'female', now)).toBe('Filly')
    expect(lifeStage('Horse', '2023-03-01', 'female_spayed', now)).toBe('Filly')
    expect(lifeStage('Horse', '2023-03-01', 'male_neutered', now)).toBe('Gelding')
    expect(lifeStage('Horse', '2023-03-01', null, now)).toBe('Young horse')
    expect(lifeStage('Horse', '2015-03-01', 'male', now)).toBe('Stallion')
    expect(lifeStage('Horse', '2015-03-01', 'female', now)).toBe('Mare')
    expect(lifeStage('Horse', '2015-03-01', 'male_neutered', now)).toBe('Gelding')
    expect(lifeStage('Horse', '2015-03-01', 'unknown', now)).toBe('Adult')
    expect(lifeStage('Horse', '2006-10-11', 'female', now)).toBe('Senior') // 20 y
  })
  it('is null without cited bands or a birth date', () => {
    expect(lifeStage('Fish', '2025-01-01', null, now)).toBe(null)
    expect(lifeStage('Bird', '2020-01-01', 'male', now)).toBe(null)
    expect(lifeStage('Dog', null, null, now)).toBe(null)
    expect(lifeStage('Dog', '2027-01-01', null, now)).toBe(null) // not born yet
    expect(lifeStage('Dog', 'soon', null, now)).toBe(null)
  })
  it('gives an archived pet no stage', () => {
    const pet = { species: 'Cat', birth_date: '2015-12-04', sex: 'male_neutered', archived_at: null }
    expect(petLifeStage(pet, now)).toBe('Senior')
    expect(petLifeStage({ ...pet, archived_at: '2026-10-20' }, now)).toBe(null) // Rocco
  })
})

describe('sexAndStageTags (#72, #73)', () => {
  const now = new Date(2026, 9, 11)
  const horse = (sex: string | null, birth_date: string) => ({ species: 'Horse', birth_date, sex })
  it('never repeats itself for horses', () => {
    expect(sexAndStageTags(horse('male_neutered', '2015-01-01'), false, now)).toEqual({ sex: 'Gelding', stage: null })
    expect(sexAndStageTags(horse('male', '2023-04-01'), false, now)).toEqual({ sex: null, stage: 'Colt' }) // not "Stallion"
    expect(sexAndStageTags(horse('female', '2023-04-01'), false, now)).toEqual({ sex: null, stage: 'Filly' })
    expect(sexAndStageTags(horse('female_spayed', '2023-04-01'), false, now)).toEqual({ sex: 'Spayed mare', stage: 'Filly' })
    expect(sexAndStageTags(horse('male', '2026-03-01'), false, now)).toEqual({ sex: 'Stallion', stage: 'Foal' })
  })
  it('shows both for dogs and cats, honours breeder terms, and hides unknown', () => {
    const lulu = { species: 'Dog', birth_date: '2025-11-04', sex: 'female' }
    expect(sexAndStageTags(lulu, false, now)).toEqual({ sex: 'Female', stage: 'Young adult' })
    expect(sexAndStageTags(lulu, true, now)).toEqual({ sex: 'Bitch', stage: 'Young adult' })
    expect(sexAndStageTags({ ...lulu, sex: 'unknown' }, false, now)).toEqual({ sex: null, stage: 'Young adult' })
    expect(sexAndStageTags({ ...lulu, sex: null, birth_date: null }, false, now)).toEqual({ sex: null, stage: null })
  })
})

describe('andList / petColors (#75)', () => {
  it('joins names as prose', async () => {
    const { andList } = await import('../src/format')
    expect(andList([])).toBe('')
    expect(andList(['Pedro'])).toBe('Pedro')
    expect(andList(['Pedro', 'Lulu'])).toBe('Pedro and Lulu')
    expect(andList(['Pedro', 'Lulu', 'Kipper'])).toBe('Pedro, Lulu and Kipper')
  })
  it('falls back to the brand stops', async () => {
    const { petColors } = await import('../src/format')
    expect(petColors({ gradient_a: '#111111', gradient_b: '#222222' })).toEqual(['#111111', '#222222'])
    expect(petColors(null)).toEqual(['#F2624C', '#DB4E38'])
  })
})
