typeset -gi _LMC_IN_FLIGHT=0
typeset -gi _LMC_EVENT_FD=-1
typeset -gi _LMC_COMMAND_PID=-1
typeset -g _LMC_ORIGINAL_BUFFER=""
typeset -g _LMC_OUTPUT_FILE=""
typeset -g _LMC_ERROR_FILE=""
typeset -g _LMC_STATUS_FILE=""
typeset -ga _LMC_LOADING_FRAMES=('.  ' '.. ' '...')

_lmc_default_config_path() {
  emulate -L zsh
  local config_root="${XDG_CONFIG_HOME:-$HOME/.config}"
  print -r -- "${config_root}/lmcomplete/config.toml"
}

_lmc_missing_provider_message() {
  emulate -L zsh
  print -r -- "Set OPENROUTER_API_KEY or configure $(_lmc_default_config_path)"
}

_lmc_has_provider_config() {
  emulate -L zsh

  if [[ -n "${OPENROUTER_API_KEY:-}" ]]; then
    return 0
  fi

  local config_path="$(_lmc_default_config_path)"
  [[ -r "$config_path" ]] || return 1

  command grep -Eq "^[[:space:]]*api_key[[:space:]]*=[[:space:]]*([\"'][^\"']+[\"'])" "$config_path"
}

_lmc_loading_indicator() {
  emulate -L zsh
  local frame="${1:-...}"
  local grey=$'\033[90m'
  local reset=$'\033[0m'
  print -nr -- "${grey} [lmc ${frame}]${reset}"
}

_lmc_loading_message() {
  emulate -L zsh
  local frame="${1:-...}"
  print -r -- "[lmc ${frame}]"
}

_lmc_show_loading_state() {
  emulate -L zsh
  local frame="${1:-...}"
  POSTDISPLAY="$(_lmc_loading_indicator "$frame")"
  zle -M -- "$(_lmc_loading_message "$frame")"
}

_lmc_command_finished() {
  emulate -L zsh

  local command_pid="$1"
  (( command_pid >= 0 )) || return 1

  local state
  state="$(command ps -o stat= -p "$command_pid" 2>/dev/null | tr -d '[:space:]')"
  [[ -z "$state" || "$state" == Z* ]]
}

_lmc_has_exit_code() {
  emulate -L zsh

  [[ -n "$_LMC_STATUS_FILE" && -s "$_LMC_STATUS_FILE" ]] || return 1
  command grep -Eq '^[0-9]+$' "$_LMC_STATUS_FILE"
}

_lmc_expand_ready() {
  emulate -L zsh

  _lmc_has_exit_code || _lmc_command_finished "$1"
}

_lmc_reset_state() {
  emulate -L zsh

  if (( _LMC_EVENT_FD >= 0 )); then
    zle -F "$_LMC_EVENT_FD"
    exec {_LMC_EVENT_FD}<&-
    _LMC_EVENT_FD=-1
  fi

  POSTDISPLAY=""
  _LMC_IN_FLIGHT=0
  _LMC_COMMAND_PID=-1

  if [[ -n "$_LMC_OUTPUT_FILE" ]]; then
    command rm -f -- "$_LMC_OUTPUT_FILE"
  fi
  if [[ -n "$_LMC_ERROR_FILE" ]]; then
    command rm -f -- "$_LMC_ERROR_FILE"
  fi
  if [[ -n "$_LMC_STATUS_FILE" ]]; then
    command rm -f -- "$_LMC_STATUS_FILE"
  fi

  _LMC_OUTPUT_FILE=""
  _LMC_ERROR_FILE=""
  _LMC_STATUS_FILE=""
}

_lmc_stream_expand() {
  emulate -L zsh

  local command_pid="$1"
  local status_file="$2"
  local frame_index=1
  local last_frame="${_LMC_LOADING_FRAMES[1]}"

  while ! _lmc_expand_ready "$command_pid" "$status_file"; do
    last_frame="${_LMC_LOADING_FRAMES[$frame_index]}"
    printf 'tick\t%s\n' "$last_frame"
    frame_index=$(( frame_index % ${#_LMC_LOADING_FRAMES[@]} + 1 ))
    sleep 0.12
  done

  printf 'tick\t%s\n' "$last_frame"
}

_lmc_apply_result() {
  emulate -L zsh

  local output="$1"

  if [[ "$output" == "# WARNING: destructive command"$'\n'* ]]; then
    BUFFER="${output#*$'\n'}"
    zle -M -- "WARNING: destructive command"
  else
    BUFFER="$output"
    zle -M ""
  fi

  CURSOR=${#BUFFER}
}

_lmc_finish_expand() {
  emulate -L zsh

  local exit_code="$1"
  local output=""
  local error=""

  if [[ -n "$_LMC_OUTPUT_FILE" && -f "$_LMC_OUTPUT_FILE" ]]; then
    output="$(<"$_LMC_OUTPUT_FILE")"
  fi

  if [[ -n "$_LMC_ERROR_FILE" && -f "$_LMC_ERROR_FILE" ]]; then
    error="$(<"$_LMC_ERROR_FILE")"
  fi

  _lmc_reset_state

  if [[ "$exit_code" == "0" ]]; then
    _lmc_apply_result "$output"
  else
    BUFFER="$_LMC_ORIGINAL_BUFFER"
    CURSOR=${#BUFFER}
    zle -M -- "${error:-lmc expansion failed}"
  fi

  zle -R
}

_lmc_finish_if_ready() {
  emulate -L zsh

  local exit_code

  if _lmc_has_exit_code; then
    exit_code="$(<"$_LMC_STATUS_FILE")"
  else
    if ! _lmc_command_finished "$_LMC_COMMAND_PID"; then
      return 1
    fi

    local command_pid="$_LMC_COMMAND_PID"
    _LMC_COMMAND_PID=-1

    if wait "$command_pid" 2>/dev/null; then
      exit_code=0
    else
      exit_code=$?
    fi
  fi

  _LMC_COMMAND_PID=-1
  _lmc_finish_expand "$exit_code"
}

_lmc_handle_expand_event() {
  emulate -L zsh

  local fd="$1"
  local event payload

  if ! IFS=$'\t' read -r event payload <&"$fd"; then
    if ! _lmc_finish_if_ready; then
      _lmc_finish_expand 1
    fi
    return 0
  fi

  while :; do
    case "$event" in
      tick)
        _lmc_show_loading_state "$payload"
        zle -R
        if _lmc_finish_if_ready; then
          return 0
        fi
        ;;
      done)
        _lmc_finish_expand "$payload"
        return 0
        ;;
    esac

    if ! IFS=$'\t' read -r -t 0 event payload <&"$fd"; then
      if _lmc_finish_if_ready; then
        return 0
      fi
      return 0
    fi
  done
}

_lmc_expand_buffer() {
  emulate -L zsh

  if (( _LMC_IN_FLIGHT )); then
    zle -M -- "lmc expansion already in progress"
    zle -R
    return 0
  fi

  local raw_input="$BUFFER"
  if [[ -z "$raw_input" ]]; then
    zle redisplay
    return 0
  fi

  if ! _lmc_has_provider_config; then
    zle -M -- "$(_lmc_missing_provider_message)"
    zle -R
    return 0
  fi

  local input="$raw_input"
  if [[ "$input" == '?? '* ]]; then
    input="${input#\?\? }"
  fi

  _LMC_ORIGINAL_BUFFER="$BUFFER"
  _LMC_OUTPUT_FILE="$(mktemp "${TMPDIR:-/tmp}/lmc-output.XXXXXX")" || return 1
  _LMC_ERROR_FILE="$(mktemp "${TMPDIR:-/tmp}/lmc-error.XXXXXX")" || {
    command rm -f -- "$_LMC_OUTPUT_FILE"
    _LMC_OUTPUT_FILE=""
    return 1
  }
  _LMC_STATUS_FILE="$(mktemp "${TMPDIR:-/tmp}/lmc-status.XXXXXX")" || {
    command rm -f -- "$_LMC_OUTPUT_FILE" "$_LMC_ERROR_FILE"
    _LMC_OUTPUT_FILE=""
    _LMC_ERROR_FILE=""
    return 1
  }

  command rm -f -- "$_LMC_STATUS_FILE"

  (
    command lmc expand --shell zsh -- "$input" >"$_LMC_OUTPUT_FILE" 2>"$_LMC_ERROR_FILE"
    print -r -- "$?" >|"$_LMC_STATUS_FILE"
  ) &!

  _LMC_COMMAND_PID=-1
  _LMC_IN_FLIGHT=1
  _lmc_show_loading_state
  exec {_LMC_EVENT_FD}< <(_lmc_stream_expand "$_LMC_COMMAND_PID" "$_LMC_STATUS_FILE")
  zle -F -w "$_LMC_EVENT_FD" lmc-handle-expand-event
  zle -R
}

_lmc_tab_expand_or_complete() {
  emulate -L zsh

  if [[ "$BUFFER" == '?? '* ]]; then
    _lmc_expand_buffer
  else
    zle expand-or-complete
  fi
}

zle -N lmc-expand-buffer _lmc_expand_buffer
zle -N lmc-handle-expand-event _lmc_handle_expand_event
zle -N lmc-tab-expand-or-complete _lmc_tab_expand_or_complete

bindkey '^[[Z' lmc-expand-buffer
bindkey '^I' lmc-tab-expand-or-complete
