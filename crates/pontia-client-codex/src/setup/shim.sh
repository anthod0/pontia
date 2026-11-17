#!/bin/sh
# Pontia Codex launcher
# Resolve the native installation on every invocation, including after upgrades.
remaining=${PATH-}
native=
while :; do
    directory=${remaining%%:*}
    candidate=${directory:-.}/codex
    if [ -f "$candidate" ] && [ -x "$candidate" ] && ! [ "$candidate" -ef "$0" ]; then
        marker=
        { IFS= read -r first; IFS= read -r marker; } < "$candidate"
        if [ "$marker" != '# Pontia Codex launcher' ]; then
            native=$candidate
            break
        fi
    fi
    case $remaining in *:*) remaining=${remaining#*:} ;; *) break ;; esac
done
if [ -z "$native" ]; then
    printf '%s\n' 'Original Codex executable not found on PATH.' >&2
    exit 127
fi

use_proxy() {
    entry=root
    explicit_cd=false
    workspace=$PWD
    while [ "$#" -gt 0 ]; do
        case $1 in
            --) return 0 ;;
            --remote|--remote=*|--no-daemon|--help|--version|-h|-V) return 1 ;;
            -C|--cd)
                [ "$#" -ge 2 ] || return 1
                explicit_cd=true
                workspace=$2
                shift 2
                continue ;;
            --cd=*) explicit_cd=true; workspace=${1#--cd=} ;;
            -C=*) explicit_cd=true; workspace=${1#-C=} ;;
            -C?*) explicit_cd=true; workspace=${1#-C} ;;
            -c|--config|--enable|--disable|--remote-auth-token-env|-i|--image|-m|--model|--local-provider|-p|--profile|-s|--sandbox|--add-dir|-a|--ask-for-approval)
                [ "$#" -ge 2 ] || return 1
                shift 2
                continue ;;
            --*=*|-c?*|-i?*|-m?*|-p?*|-s?*|-a?*) ;;
            --oss|--strict-config|--approve-for-me|--dangerously-bypass-approvals-and-sandbox|--dangerously-bypass-hook-trust|--worktree|--search|--no-alt-screen|--last|--all|--include-non-interactive) ;;
            -*) return 1 ;;
            *)
                if [ "$entry" = root ]; then
                    case $1 in
                        agents|resume|fork) entry=tui ;;
                        exec|e|review|login|logout|mcp|mcp-server|plugin|app|app-server|remote-control|completion|update|doctor|sandbox|debug|apply|a|queue|archive|delete|migrate-rollouts|migrate-nfs-db|unarchive|cloud|exec-server|features|help) return 1 ;;
                        *) entry=prompt ;;
                    esac
                fi ;;
        esac
        shift
    done
    return 0
}
if use_proxy "$@" && PONTIA_HOME=__PONTIA_HOME__ pontia workspace-check -- "$workspace" >/dev/null 2>&1; then
    if [ "$explicit_cd" = false ]; then
        set -- --cd "$PWD" "$@"
    fi
    exec "$native" --remote __PONTIA_REMOTE__ "$@"
fi
exec "$native" "$@"
