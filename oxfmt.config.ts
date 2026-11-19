// oxfmt's configuration lies in typescript/. This root keeps editors that run
// oxfmt from here to typescript/, whose configuration then applies
export default {
    ignorePatterns: ['/*', '!/typescript']
};
