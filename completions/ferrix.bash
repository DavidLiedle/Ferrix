#!/bin/bash
# Bash completion script for Ferrix

_ferrix() {
    local cur prev opts
    COMPREPLY=()
    cur="${COMP_WORDS[COMP_CWORD]}"
    prev="${COMP_WORDS[COMP_CWORD-1]}"

    # Main commands
    local commands="new attach detach list kill kill-server restore save-snapshot restore-snapshot \
                    list-snapshots export-snapshot import-snapshot server connect plugin version help"

    # Global options
    local global_opts="-h --help -V --version"

    case "${prev}" in
        ferrix)
            COMPREPLY=( $(compgen -W "${commands} ${global_opts}" -- ${cur}) )
            return 0
            ;;
        new)
            local opts="-s --session -d --detached -c --command --working-dir"
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        attach)
            local opts="-t --target --force --read-only"
            if [[ ${cur} == -* ]]; then
                COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            else
                # Complete with available sessions
                local sessions=$(ferrix list 2>/dev/null | cut -d: -f1)
                COMPREPLY=( $(compgen -W "${sessions}" -- ${cur}) )
            fi
            return 0
            ;;
        kill)
            local opts="-t --target"
            if [[ ${cur} == -* ]]; then
                COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            else
                # Complete with available sessions
                local sessions=$(ferrix list 2>/dev/null | cut -d: -f1)
                COMPREPLY=( $(compgen -W "${sessions}" -- ${cur}) )
            fi
            return 0
            ;;
        save-snapshot|restore-snapshot)
            if [[ ${cur} == -* ]]; then
                local opts="--name --description --session-id"
                COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            else
                # Complete with available sessions
                local sessions=$(ferrix list 2>/dev/null | cut -d: -f1)
                COMPREPLY=( $(compgen -W "${sessions}" -- ${cur}) )
            fi
            return 0
            ;;
        export-snapshot|import-snapshot)
            local opts="--output --input"
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        server)
            local opts="--bind --cert --key --auth-mode --auth-token --foreground"
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        connect)
            local opts="--list --password --token --cert"
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        plugin)
            local subcommands="install uninstall list enable disable test"
            COMPREPLY=( $(compgen -W "${subcommands}" -- ${cur}) )
            return 0
            ;;
        version)
            local opts="--commit --build-date --verbose"
            COMPREPLY=( $(compgen -W "${opts}" -- ${cur}) )
            return 0
            ;;
        help)
            COMPREPLY=( $(compgen -W "${commands}" -- ${cur}) )
            return 0
            ;;
        -s|--session|-t|--target)
            # Complete with available sessions
            local sessions=$(ferrix list 2>/dev/null | cut -d: -f1)
            COMPREPLY=( $(compgen -W "${sessions}" -- ${cur}) )
            return 0
            ;;
        --working-dir)
            # Complete with directories
            COMPREPLY=( $(compgen -d -- ${cur}) )
            return 0
            ;;
        -c|--command)
            # Complete with available commands
            COMPREPLY=( $(compgen -c -- ${cur}) )
            return 0
            ;;
        *)
            ;;
    esac

    # Default to commands if nothing matches
    if [[ ${cur} == -* ]]; then
        COMPREPLY=( $(compgen -W "${global_opts}" -- ${cur}) )
    else
        COMPREPLY=( $(compgen -W "${commands}" -- ${cur}) )
    fi
}

complete -F _ferrix ferrix