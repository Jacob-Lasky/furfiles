-- Pet sex becomes a closed set (#72): male | female | male_neutered |
-- female_spayed | unknown (`SEXES` in models.rs). It was free text, so the same
-- fact was typed four ways ("Male · neutered", "Neutered male", "Gelding",
-- "Female") and nothing could rely on it.
--
-- The free text already stored is mapped onto the set with the SAME rules as
-- `sex::sex_from_text` in Rust (word matches, case-insensitive):
--   an intact word (intact, entire, not, no, planned, due, …) rules out the two
--   sterilised values below: "Female, not spayed" is an intact female
--   a spay word, "FS"/"SF", or a neuter word with a female word -> female_spayed
--   a neuter / castration / gelding / altered word, "MN"/"MC"   -> male_neutered
--   female, F, mare, queen, bitch, filly, …                      -> female
--   male, M, stallion, tom, colt, stud, …                        -> male
--   anything else                                                -> unknown
-- `tests/api.rs::sex_migration_matches_the_rust_mapping` runs this file and the
-- Rust function over the same strings (every word of the Rust vocabulary plus
-- real values) and requires them to agree. Once applied, this file is FROZEN:
-- sqlx checksums it and refuses to start if it changes, so never edit it to
-- follow a later change to the Rust rules.
--
-- NULL stays NULL: "not recorded" is a different fact from an owner choosing
-- `unknown`, and the API keeps treating blank as NULL like every other field.
-- A blank string becomes NULL for the same reason.
--
-- Nothing is silently lost: every pet's original text, and what it became, is
-- kept in `pet_sex_legacy`. Nothing reads it; it is the record of this change.

CREATE TABLE pet_sex_legacy (
    pet_id      TEXT PRIMARY KEY REFERENCES pets(id) ON DELETE CASCADE,
    original    TEXT NOT NULL,
    migrated_to TEXT NOT NULL,
    migrated_at TEXT NOT NULL DEFAULT (datetime('now'))
);

-- A scratch table holding each value as space-separated WORDS: lower-cased,
-- punctuation and odd spaces (non-breaking, thin, zero-width) turned into plain
-- spaces, runs collapsed, padded with one space each side, so a word test is
-- `LIKE '% word %'` and a two-word test is `LIKE '% f s %'`. Done in short
-- steps rather than one deeply nested replace(), which overflows the parser
-- stack of some SQLite builds.
CREATE TABLE _pet_sex_words (pet_id TEXT PRIMARY KEY, w TEXT NOT NULL);
INSERT INTO _pet_sex_words (pet_id, w)
SELECT id, lower(sex) FROM pets WHERE sex IS NOT NULL AND trim(sex) != '';
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '·', ' '), '•', ' '), '/', ' '), '\', ' '), '-', ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '–', ' '), '—', ' '), ',', ' '), '.', ' '), '(', ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, ')', ' '), '_', ' '), ';', ' '), ':', ' '), '&', ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '+', ' '), '|', ' '), '?', ' '), '#', ' '), '*', ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '!', ' '), '"', ' '), '''', ' '), '[', ' '), ']', ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '<', ' '), '>', ' '), '=', ' '), char(9), ' '), char(10), ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, char(13), ' '), char(160), ' '), char(8194), ' '), char(8195), ' '), char(8201), ' ');
UPDATE _pet_sex_words SET w = replace(replace(replace(w, char(8202), ' '), char(8203), ' '), char(8239), ' ');
-- Collapse runs of spaces (each pass halves a run; five cover 32), then pad.
UPDATE _pet_sex_words SET w = replace(replace(replace(replace(replace(w, '  ', ' '), '  ', ' '), '  ', ' '), '  ', ' '), '  ', ' ');
UPDATE _pet_sex_words SET w = ' ' || trim(w) || ' ';

INSERT INTO pet_sex_legacy (pet_id, original, migrated_to)
SELECT p.id, p.sex,
  CASE
    WHEN NOT (w LIKE '% intact %' OR w LIKE '% entire %' OR w LIKE '% not %' OR w LIKE '% no %'
        OR w LIKE '% non %' OR w LIKE '% planned %' OR w LIKE '% scheduled %'
        OR w LIKE '% pending %' OR w LIKE '% due %')
     AND (w LIKE '% spay %' OR w LIKE '% spayed %' OR w LIKE '% spaying %'
        OR w LIKE '% ovariohysterectomy %' OR w LIKE '% ovariectomy %' OR w LIKE '% ohe %'
        OR w LIKE '% ovh %' OR w LIKE '% fs %' OR w LIKE '% sf %' OR w LIKE '% f s %'
        OR w LIKE '% s f %'
       OR ((w LIKE '% neuter %' OR w LIKE '% neutered %' OR w LIKE '% neutering %'
        OR w LIKE '% castrate %' OR w LIKE '% castrated %' OR w LIKE '% castration %'
        OR w LIKE '% gelding %' OR w LIKE '% gelded %' OR w LIKE '% geld %'
        OR w LIKE '% orchiectomy %' OR w LIKE '% orchidectomy %' OR w LIKE '% mn %'
        OR w LIKE '% nm %' OR w LIKE '% mc %' OR w LIKE '% cm %' OR w LIKE '% altered %'
        OR w LIKE '% desexed %' OR w LIKE '% m n %' OR w LIKE '% m c %')
           AND (w LIKE '% female %' OR w LIKE '% f %' OR w LIKE '% fi %' OR w LIKE '% mare %'
        OR w LIKE '% queen %' OR w LIKE '% bitch %' OR w LIKE '% filly %'
        OR w LIKE '% hen %' OR w LIKE '% doe %' OR w LIKE '% jill %' OR w LIKE '% jenny %')))
      THEN 'female_spayed'
    WHEN NOT (w LIKE '% intact %' OR w LIKE '% entire %' OR w LIKE '% not %' OR w LIKE '% no %'
        OR w LIKE '% non %' OR w LIKE '% planned %' OR w LIKE '% scheduled %'
        OR w LIKE '% pending %' OR w LIKE '% due %')
     AND (w LIKE '% neuter %' OR w LIKE '% neutered %' OR w LIKE '% neutering %'
        OR w LIKE '% castrate %' OR w LIKE '% castrated %' OR w LIKE '% castration %'
        OR w LIKE '% gelding %' OR w LIKE '% gelded %' OR w LIKE '% geld %'
        OR w LIKE '% orchiectomy %' OR w LIKE '% orchidectomy %' OR w LIKE '% mn %'
        OR w LIKE '% nm %' OR w LIKE '% mc %' OR w LIKE '% cm %' OR w LIKE '% altered %'
        OR w LIKE '% desexed %' OR w LIKE '% m n %' OR w LIKE '% m c %')
      THEN 'male_neutered'
    WHEN w LIKE '% female %' OR w LIKE '% f %' OR w LIKE '% fi %' OR w LIKE '% mare %'
        OR w LIKE '% queen %' OR w LIKE '% bitch %' OR w LIKE '% filly %'
        OR w LIKE '% hen %' OR w LIKE '% doe %' OR w LIKE '% jill %' OR w LIKE '% jenny %' OR w LIKE '% f i %'
      THEN 'female'
    WHEN w LIKE '% male %' OR w LIKE '% m %' OR w LIKE '% mi %' OR w LIKE '% stallion %'
        OR w LIKE '% tom %' OR w LIKE '% colt %' OR w LIKE '% stud %' OR w LIKE '% buck %'
        OR w LIKE '% boar %' OR w LIKE '% jack %' OR w LIKE '% rooster %'
        OR w LIKE '% cock %' OR w LIKE '% entire %' OR w LIKE '% m i %'
      THEN 'male'
    ELSE 'unknown'
  END
FROM pets p JOIN _pet_sex_words s ON s.pet_id = p.id;

DROP TABLE _pet_sex_words;

UPDATE pets
SET sex = (SELECT migrated_to FROM pet_sex_legacy l WHERE l.pet_id = pets.id)
WHERE id IN (SELECT pet_id FROM pet_sex_legacy);

UPDATE pets SET sex = NULL WHERE trim(sex) = '';
