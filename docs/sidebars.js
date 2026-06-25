/** @type {import('@docusaurus/plugin-content-docs').SidebarsConfig} */
const sidebars = {
  docsSidebar: [
    'intro',
    {
      type: 'category',
      label: 'Architecture',
      items: ['architecture', 'data-flow', 'retry-control'],
    },
    {
      type: 'category',
      label: 'Operations',
      items: ['setup', 'runbook', 'cli'],
    },
    {
      type: 'category',
      label: 'Reference',
      items: ['api-reference', 'failure-classification', 'configuration'],
    },
    'verification',
  ],
};

export default sidebars;
