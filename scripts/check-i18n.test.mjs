import { describe, expect, it } from 'vitest';
import { readCatalogs, validateCatalogs } from './check-i18n.mjs';

describe('i18n catalog guard', () => {
  it('accepts the bundled vi/en/ja catalogs', () => {
    expect(validateCatalogs(readCatalogs())).toEqual([]);
  });

  it('reports missing, extra, malformed, empty, and forbidden legacy keys', () => {
    const catalogs = {
      en: { 'home.header.title': 'Home' },
      vi: { 'home.header.extra': 'Thừa', 'setup.old.title': 'Cũ' },
      ja: { 'home.header.title': '' },
    };
    const errors = validateCatalogs(catalogs);

    expect(errors).toEqual(expect.arrayContaining([
      'vi: missing key home.header.title',
      'vi: extra key home.header.extra',
      'vi: extra key setup.old.title',
      'vi: forbidden legacy key setup.old.title',
      'ja: value for home.header.title must be a non-empty string',
    ]));
  });

  it('rejects keys that do not have exactly three convention segments', () => {
    const catalogs = {
      en: { 'home.header.title.extra': 'Too deep' },
      vi: { 'home.header.title.extra': 'Quá sâu' },
      ja: { 'home.header.title.extra': '深すぎます' },
    };

    expect(validateCatalogs(catalogs)).toEqual(expect.arrayContaining([
      'en: invalid key home.header.title.extra',
      'vi: invalid key home.header.title.extra',
      'ja: invalid key home.header.title.extra',
    ]));
  });
});
