const { env } = require('node:process');
env.github_action_phase = 'main';
const impl = require('./index.js');
