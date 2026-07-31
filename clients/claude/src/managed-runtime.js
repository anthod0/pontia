export function hasTmuxPaneEnvironment(env = process.env) {
    const socketPath = env.TMUX?.trim().split(",", 1)[0]?.trim();
    return Boolean(socketPath && env.TMUX_PANE?.trim());
}
