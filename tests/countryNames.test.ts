import assert from 'node:assert/strict';
import { describe, test } from 'node:test';
import { getCountryName, compareCountryNames } from '../src/utils/countryNames.ts';

describe('Local Country Name Resolution and Sorting', () => {
  test('expands standard ISO 3166-1 alpha-2 codes to full country names', () => {
    assert.equal(getCountryName('RO'), 'Romania');
    assert.equal(getCountryName('ro'), 'Romania');
    assert.equal(getCountryName('US'), 'United States');
    assert.equal(getCountryName('JP'), 'Japan');
    assert.equal(getCountryName('DE'), 'Germany');
    assert.equal(getCountryName('GB'), 'United Kingdom');
    assert.equal(getCountryName('FR'), 'France');
  });

  test('handles missing or empty codes gracefully without inventing names', () => {
    assert.equal(getCountryName(null), null);
    assert.equal(getCountryName(undefined), null);
    assert.equal(getCountryName(''), null);
    assert.equal(getCountryName('  '), null);
    assert.equal(getCountryName('—'), null);
  });

  test('returns clean code for unrecognized or non-standard identifiers without inventing country', () => {
    assert.equal(getCountryName('XX'), 'XX');
    assert.equal(getCountryName('ZZ'), 'ZZ');
    assert.equal(getCountryName('CUSTOM'), 'CUSTOM');
  });

  test('sorts by displayed full country name alphabetically', () => {
    // Romania (RO) vs Germany (DE) -> Germany comes before Romania
    assert.ok(compareCountryNames('DE', 'RO') < 0);
    assert.ok(compareCountryNames('RO', 'DE') > 0);

    // United States (US) vs Australia (AU) -> Australia comes before United States
    assert.ok(compareCountryNames('AU', 'US') < 0);
    assert.ok(compareCountryNames('US', 'AU') > 0);

    // Missing entries sort last
    assert.ok(compareCountryNames('RO', null) < 0);
    assert.ok(compareCountryNames(null, 'RO') > 0);
    assert.equal(compareCountryNames(null, undefined), 0);
  });
});
