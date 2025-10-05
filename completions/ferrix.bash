#!/usr/bin/env bash
# Bash completion script for ferrix

_ferrix() {
    local cur prev opts base
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Main commands
    opts="new attach detach list kill rename server help version \
          save-snapshot load-snapshot list-snapshots \
          new-window select-window rename-window kill-window list-windows \
          split-pane select-pane kill-pane resize-pane \
          apply-layout save-layout cycle-layout list-layouts \
          enter-copy-mode \
          plugin commit-session branch checkout merge log \
          enable-autosave disable-autosave"

    # Sub-command options
    case "${prev}" in
        new|attach)
            local sessions=$(ferrix list 2>/dev/null | awk '{print $1}')
            COMPREPLY=($(compgen -W "${sessions} -s -t --session --target" -- ${cur}))
            return 0
            ;;
        kill|rename|save-snapshot)
            local sessions=$(ferrix list 2>/dev/null | awk '{print $1}')
            COMPREPLY=($(compgen -W "${sessions}" -- ${cur}))
            return 0
            ;;
        apply-layout)
            local layouts="single vsplit hsplit main-left main-right main-top main-bottom \
                          3v 3h 2x2 ide 3x2"
            COMPREPLY=($(compgen -W "${layouts}" -- ${cur}))
            return 0
            ;;
        split-pane|split)
            COMPREPLY=($(compgen -W "-h -v --horizontal --vertical" -- ${cur}))
            return 0
            ;;
        resize-pane)
            COMPREPLY=($(compgen -W "-U -D -L -R --up --down --left --right" -- ${cur}))
            return 0
            ;;
        plugin)
            COMPREPLY=($(compgen -W "install uninstall update list search info" -- ${cur}))
            return 0
            ;;
        branch|checkout|merge)
            local branches=$(ferrix branch --list 2>/dev/null | awk '{print $1}')
            COMPREPLY=($(compgen -W "${branches}" -- ${cur}))
            return 0
            ;;
        --features)
            COMPREPLY=($(compgen -W "gpu" -- ${cur}))
            return 0
            ;;
    esac

    # Handle options starting with dash
    if [[ ${cur} == -* ]]; then
        local global_opts="-h --help -v --version -c --config -l --log-level"
        COMPREPLY=($(compgen -W "${global_opts}" -- ${cur}))
        return 0
    fi

    COMPREPLY=($(compgen -W "${opts}" -- ${cur}))
    return 0
}

complete -F _ferrix ferrix