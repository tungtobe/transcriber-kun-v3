#!/usr/bin/env node
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));
export const LOCALES = ['vi', 'en', 'ja'];
export const KEY_PATTERN = /^[a-z][a-zA-Z0-9]*\.[a-z][a-zA-Z0-9]*\.[a-z][a-zA-Z0-9]*$/;
export const FORBIDDEN_SEGMENTS = new Set(['setup', 'whisper', 'whisperx', 'copilot', 'update', 'updates']);

export function validateCatalogs(catalogs) {
  const errors = [];
  const referenceKeys = Object.keys(catalogs.en ?? {}).sort();
  const referenceSet = new Set(referenceKeys);

  for (const locale of LOCALES) {
    const catalog = catalogs[locale];
    if (!catalog || typeof catalog !== 'object' || Array.isArray(catalog)) {
      errors.push(`${locale}: catalog must be a flat object`);
      continue;
    }

    const keys = Object.keys(catalog).sort();
    for (const key of referenceKeys) {
      if (!Object.hasOwn(catalog, key)) errors.push(`${locale}: missing key ${key}`);
    }
    for (const key of keys) {
      if (!referenceSet.has(key)) errors.push(`${locale}: extra key ${key}`);
      if (!KEY_PATTERN.test(key)) errors.push(`${locale}: invalid key ${key}`);
      if (key.split('.').some((segment) => FORBIDDEN_SEGMENTS.has(segment.toLowerCase()))) {
        errors.push(`${locale}: forbidden legacy key ${key}`);
      }
      if (typeof catalog[key] !== 'string' || catalog[key].trim() === '') {
        errors.push(`${locale}: value for ${key} must be a non-empty string`);
      }
    }
  }

  return errors;
}

export function readCatalogs(root = ROOT) {
  return Object.fromEntries(LOCALES.map((locale) => [
    locale,
    JSON.parse(readFileSync(join(root, 'src', 'i18n', `${locale}.json`), 'utf8')),
  ]));
}

export function runI18nCheck() {
  const errors = validateCatalogs(readCatalogs());
  if (errors.length) {
    console.error('check:i18n FAIL');
    for (const error of errors) console.error(`  ${error}`);
    return 1;
  }
  console.log('check:i18n ok — vi/en/ja key parity and naming rules pass.');
  return 0;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  process.exit(runI18nCheck());
}
