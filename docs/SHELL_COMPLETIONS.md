# Shell Completions for Ferrix

Ferrix supports shell completions for bash, zsh, fish, powershell, and elvish.

## Installation

### Generating Completions

Use the `completions` command to generate completion scripts:

```bash
# Generate completions for your shell
ferrix completions bash --output ferrix_completions.bash
ferrix completions zsh --output _ferrix
ferrix completions fish --output ferrix.fish
ferrix completions powershell --output ferrix.ps1
ferrix completions elvish --output ferrix.elv
```

### Bash

**Option 1: User installation (recommended)**

```bash
# Create completions directory if it doesn't exist
mkdir -p ~/.local/share/bash-completion/completions

# Generate and install completion script
ferrix completions bash --output ~/.local/share/bash-completion/completions/ferrix

# Restart your shell or source the file
source ~/.local/share/bash-completion/completions/ferrix
```

**Option 2: System-wide installation**

```bash
# Generate completion script (requires sudo)
sudo ferrix completions bash --output /etc/bash_completion.d/ferrix

# Restart your shell
```

### Zsh

**Option 1: Using oh-my-zsh (recommended)**

```bash
# Generate completion script
ferrix completions zsh --output ~/.oh-my-zsh/custom/plugins/ferrix/_ferrix

# Restart your shell or reload configuration
source ~/.zshrc
```

**Option 2: Manual installation**

```bash
# Create completions directory if needed
mkdir -p ~/.zsh/completions

# Generate completion script
ferrix completions zsh --output ~/.zsh/completions/_ferrix

# Add to ~/.zshrc if not already present:
# fpath=(~/.zsh/completions $fpath)
# autoload -Uz compinit && compinit

# Restart your shell
```

**Option 3: System-wide installation**

```bash
# Generate completion script (requires sudo)
# Location depends on your zsh installation:
sudo ferrix completions zsh --output /usr/local/share/zsh/site-functions/_ferrix

# Restart your shell
```

### Fish

```bash
# Create completions directory
mkdir -p ~/.config/fish/completions

# Generate completion script
ferrix completions fish --output ~/.config/fish/completions/ferrix.fish

# Completions will be available immediately (no restart needed)
```

### PowerShell

**Windows PowerShell / PowerShell Core:**

```powershell
# Generate completion script
ferrix completions powershell --output ferrix.ps1

# Add to your PowerShell profile
# Find profile location: $PROFILE
# Example: C:\Users\<username>\Documents\PowerShell\Microsoft.PowerShell_profile.ps1

# Add this line to your profile:
. path\to\ferrix.ps1
```

### Elvish

```bash
# Create completions directory
mkdir -p ~/.config/elvish/lib

# Generate completion script
ferrix completions elvish --output ~/.config/elvish/lib/ferrix.elv

# Add to your elvish rc file (~/.config/elvish/rc.elv):
# use ferrix

# Restart your shell
```

## Verifying Installation

After installing completions, verify they work:

```bash
# Type 'ferrix ' and press TAB
ferrix <TAB>

# You should see available subcommands like:
# new  attach  list  kill  server  ...

# Test subcommand completion
ferrix n<TAB>    # Should complete to 'new'
ferrix a<TAB>    # Should complete to 'attach'
```

## Troubleshooting

### Bash Completions Not Working

1. **Check if bash-completion is installed:**
   ```bash
   # Debian/Ubuntu
   sudo apt-get install bash-completion

   # macOS (Homebrew)
   brew install bash-completion@2
   ```

2. **Verify bash-completion is sourced in ~/.bashrc:**
   ```bash
   # Add to ~/.bashrc if missing:
   if [ -f /etc/bash_completion ]; then
       . /etc/bash_completion
   fi
   ```

3. **Check completion file location:**
   ```bash
   # View where bash looks for completions
   echo ${BASH_COMPLETION_USER_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/bash-completion}/completions
   ```

### Zsh Completions Not Working

1. **Verify fpath includes completion directory:**
   ```zsh
   echo $fpath
   ```

2. **Rebuild completion cache:**
   ```zsh
   rm -f ~/.zcompdump
   compinit
   ```

3. **Check if completion function is loaded:**
   ```zsh
   which _ferrix
   ```

### Fish Completions Not Working

1. **Verify fish completions path:**
   ```fish
   echo $fish_complete_path
   ```

2. **Check if completion file exists:**
   ```fish
   ls ~/.config/fish/completions/ferrix.fish
   ```

3. **Reload completions:**
   ```fish
   fish_update_completions
   ```

## Completion Features

Ferrix completions provide:

- **Command completion** - All ferrix subcommands (new, attach, list, kill, etc.)
- **Option completion** - All command-line flags and options
- **Argument completion** - Context-aware argument suggestions
- **Alias support** - Completions work with command aliases (n, a, ls, k, d)

### Examples

```bash
# Complete subcommands
ferrix <TAB>
→ new, attach, list, kill, server, save-snapshot, ...

# Complete options for specific commands
ferrix new --<TAB>
→ --session, --command, --detached, --help

# Complete with aliases
ferrix n -<TAB>
→ -s, -c, -d, -h, -V

# Complete nested commands
ferrix user-management <TAB>
→ add, remove, list, update-role, change-password
```

## Updating Completions

When Ferrix is updated with new commands or options, regenerate your completions:

```bash
# For bash
ferrix completions bash --output ~/.local/share/bash-completion/completions/ferrix
source ~/.local/share/bash-completion/completions/ferrix

# For zsh
ferrix completions zsh --output ~/.zsh/completions/_ferrix
rm -f ~/.zcompdump && compinit

# For fish
ferrix completions fish --output ~/.config/fish/completions/ferrix.fish
# No reload needed - fish auto-reloads
```

## Contributing

If you find issues with completions, please report them on:
https://github.com/davidliedle/Ferrix/issues

---

**Last Updated**: 2025-10-05
**Version**: 0.11.0
