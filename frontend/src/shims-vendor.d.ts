// heic2any ships no type declarations. It's lazy-imported by imagefile.ts (the portrait editor,
// bulk import and record scan share it) and mocked in tests, so declare it as an untyped module to keep the
// type-check honest without inventing a fake signature for a 3rd-party lib.
declare module 'heic2any'
