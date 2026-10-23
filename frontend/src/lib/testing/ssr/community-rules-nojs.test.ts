import { createRequire } from 'node:module';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';
import { render } from 'svelte/server';
import RulesPage from '../../../routes/community-rules/+page.svelte';
import RegisterPage from '../../../routes/register/+page.svelte';
import { COMMUNITY_RULES_VERSION, rules } from '$lib/community-rules';

const require = createRequire(import.meta.url);
const miniProgramRules = require('../../../../../miniprogram/utils/community-rules.js') as {
  version: string;
  sections: ReadonlyArray<{ title: string; items: ReadonlyArray<string> }>;
};
const repositoryRoot = resolve(process.cwd(), '..');
const canonicalRulesDocument = readFileSync(resolve(repositoryRoot, 'docs/COMMUNITY-RULES.md'), 'utf8');
const registrationAudit = readFileSync(resolve(repositoryRoot, 'backend/src/auth/registration.rs'), 'utf8');
const bootstrapAudit = readFileSync(resolve(repositoryRoot, 'backend/src/bootstrap.rs'), 'utf8');
const miniProgramApp = JSON.parse(readFileSync(resolve(repositoryRoot, 'miniprogram/app.json'), 'utf8')) as { pages: string[] };
const miniProgramRegister = readFileSync(resolve(repositoryRoot, 'miniprogram/pages/register/register.wxml'), 'utf8');
const miniProgramRulesPage = readFileSync(resolve(repositoryRoot, 'miniprogram/pages/community-rules/community-rules.wxml'), 'utf8');
const miniProgramSitemap = JSON.parse(readFileSync(resolve(repositoryRoot, 'miniprogram/sitemap.json'), 'utf8')) as {
  rules: ReadonlyArray<{ action: string; page: string }>;
};

describe('community rules page', () => {
  it('renders every v1 rule and the version without requiring JavaScript', () => {
    const { body } = render(RulesPage, { props: {} });
    expect(body).toContain('社区规则');
    expect(body).toContain(`版本 ${COMMUNITY_RULES_VERSION}`);
    for (const section of rules) {
      expect(body).toContain(section.title);
      for (const item of section.items) expect(body).toContain(item);
    }
  });

  it('keeps version and rule text aligned across docs, web, and Mini Program', () => {
    expect(miniProgramRules.version).toBe(COMMUNITY_RULES_VERSION);
    expect(miniProgramRules.sections).toEqual(rules);
    expect(registrationAudit).toContain('community_rules_version": "v1"');
    expect(bootstrapAudit).toContain('"community_rules_version": "v1"');
    expect(miniProgramApp.pages).toContain('pages/community-rules/community-rules');
    expect(miniProgramRegister).toContain('pages/community-rules/community-rules');
    expect(miniProgramRulesPage).toContain('wx:for="{{sections}}"');
    expect(miniProgramSitemap.rules).toContainEqual({ action: 'allow', page: 'pages/community-rules/community-rules' });
    expect(canonicalRulesDocument).toContain(`# BBLBB 社区规则 ${COMMUNITY_RULES_VERSION}`);
    for (const section of rules) {
      expect(canonicalRulesDocument).toContain(`## ${section.title}`);
      for (const item of section.items) expect(canonicalRulesDocument).toContain(`- ${item}`);
    }
  });

  it('links the registration consent to the readable rules page', () => {
    const { body } = render(RegisterPage, { props: {} });
    expect(body).toContain('href="/community-rules"');
    expect(body).not.toContain('href="/posts/rules"');
  });
});
