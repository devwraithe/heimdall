// @ts-check
// `@type` JSDoc annotations allow editor autocompletion and type checking.

import {themes as prismThemes} from 'prism-react-renderer';

/** @type {import('@docusaurus/types').Config} */
const config = {
  title: 'Heimdall',
  tagline: 'Smart Solana Transaction Infrastructure Stack',
  favicon: 'img/favicon.ico',

  url: 'https://heimdall-docs.vercel.app',
  baseUrl: '/',

  organizationName: 'heimdall',
  projectName: 'heimdall',

  onBrokenLinks: 'warn',
  onBrokenMarkdownLinks: 'warn',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      /** @type {import('@docusaurus/preset-classic').Options} */
      ({
        docs: {
          sidebarPath: './sidebars.js',
          routeBasePath: '/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      }),
    ],
  ],

  markdown: {
  },

  themeConfig:
    /** @type {import('@docusaurus/preset-classic').ThemeConfig} */
    ({
      colorMode: {
        defaultMode: 'dark',
        disableSwitch: false,
        respectPrefersColorScheme: true,
      },
      navbar: {
        title: '⚡ Heimdall',
        items: [
          {
            type: 'docSidebar',
            sidebarId: 'docsSidebar',
            position: 'left',
            label: 'Documentation',
          },
          {
            href: 'https://github.com/devwraithe/heimdall',
            label: 'GitHub',
            position: 'right',
          },
        ],
      },
      footer: {
        style: 'dark',
        links: [
          {
            title: 'Documentation',
            items: [
              { label: 'Architecture', to: '/architecture' },
              { label: 'API Reference', to: '/api-reference' },
              { label: 'Verification Guide', to: '/verification' },
            ],
          },
          {
            title: 'Operations',
            items: [
              { label: 'Setup Guide', to: '/setup' },
              { label: 'Runbook', to: '/runbook' },
              { label: 'CLI Reference', to: '/cli' },
            ],
          },
        ],
        copyright: `Built for Superteam Earn — Smart Transaction Infrastructure Stack`,
      },
      prism: {
        theme: prismThemes.github,
        darkTheme: prismThemes.dracula,
        additionalLanguages: ['rust', 'toml', 'bash', 'protobuf', 'json'],
      },
    }),
};

export default config;
