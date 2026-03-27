typeset -gi _LMC_IN_FLIGHT=0
typeset -gi _LMC_EVENT_FD=-1
typeset -gi _LMC_COMMAND_PID=-1
typeset -gi _LMC_ANIMATION_PID=-1
typeset -g _LMC_ORIGINAL_BUFFER=""
typeset -g _LMC_PREVIEW_BUFFER=""
typeset -g _LMC_OUTPUT_FILE=""
typeset -g _LMC_ERROR_FILE=""
typeset -g _LMC_STATUS_FILE=""
typeset -g _LMC_EVENT_FILE=""
typeset -g _LMC_PRE_REDRAW_WIDGET="${_LMC_PRE_REDRAW_WIDGET:-}"
typeset -gi _LMC_CLEAR_MESSAGE_ON_NEXT_REDRAW=0
typeset -gi _LMC_MESSAGE_VISIBLE=0
typeset -g _LMC_EXPLAIN_BUFFER=""
typeset -gi _LMC_RUNNING_PRE_REDRAW=0
typeset -ga _LMC_LOADING_FRAMES=('▏' '▎' '▍' '▌' '▋' '▊' '▉' '█' '▉' '▊' '▋' '▌' '▍' '▎')

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
  local frame="${1:-▌}"
  local accent=$'\033[38;5;117m'
  local reset=$'\033[0m'
  print -nr -- $'\t'"${accent}${frame}${reset}"
}

_lmc_loading_message() {
  emulate -L zsh
  local frame="${1:-▌}"
  print -r -- "[${frame}]"
}

_lmc_explain_indicator() {
  emulate -L zsh
  local explanation="$1"
  local grey=$'\033[90m'
  local reset=$'\033[0m'
  print -nr -- " ${grey}# ${explanation}${reset}"
}

_lmc_error_message() {
  emulate -L zsh
  local text="$1"
  local red=$'\033[31m'
  local reset=$'\033[0m'
  print -r -- "${red}${text}${reset}"
}

_lmc_clear_message() {
  emulate -L zsh
  _LMC_CLEAR_MESSAGE_ON_NEXT_REDRAW=0
  if (( _LMC_MESSAGE_VISIBLE )); then
    zle -M ""
    _LMC_MESSAGE_VISIBLE=0
  fi
}

_lmc_clear_explanation() {
  emulate -L zsh
  POSTDISPLAY=""
  _LMC_EXPLAIN_BUFFER=""
  _lmc_clear_message
}

_lmc_soft_redraw() {
  emulate -L zsh
  zle -R
}

_lmc_invalidate_display() {
  emulate -L zsh
  zle -I
}

_lmc_finalize_widget_display() {
  emulate -L zsh

  case "${WIDGET:-}" in
    lmc-handle-expand-event)
      _lmc_invalidate_display
      _lmc_soft_redraw
      ;;
    *)
      zle reset-prompt 2>/dev/null || _lmc_soft_redraw
      ;;
  esac
}

_lmc_capture_previous_line_pre_redraw() {
  emulate -L zsh

  local widget=""
  local definition=""

  definition="$(zle -l -L zle-line-pre-redraw 2>/dev/null)" || definition=""
  if [[ "$definition" == "zle -N zle-line-pre-redraw "* ]]; then
    widget="${definition#zle -N zle-line-pre-redraw }"
  fi

  if [[ -z "$widget" && -n "${parameters[WIDGET_HANDLERS]:-}" ]]; then
    widget="${WIDGET_HANDLERS[zle-line-pre-redraw]:-}"
  fi

  if [[ -n "$widget" && "$widget" != "_lmc_line_pre_redraw" ]]; then
    _LMC_PRE_REDRAW_WIDGET="$widget"
  fi
}

_lmc_run_previous_line_pre_redraw() {
  emulate -L zsh

  local widget="${_LMC_PRE_REDRAW_WIDGET:-}"
  [[ -n "$widget" && "$widget" != "_lmc_line_pre_redraw" ]] || return 0
  (( _LMC_RUNNING_PRE_REDRAW )) && return 0

  _LMC_RUNNING_PRE_REDRAW=1
  "$widget" 2>/dev/null
  _LMC_RUNNING_PRE_REDRAW=0
}

_lmc_show_error_message() {
  emulate -L zsh
  local text="${1:-lmc expansion failed}"
  _LMC_CLEAR_MESSAGE_ON_NEXT_REDRAW=1
  _LMC_MESSAGE_VISIBLE=1
  zle -M -- "$(_lmc_error_message "$text")"
}

_lmc_show_loading_state() {
  emulate -L zsh
  local frame="${1:-${_LMC_LOADING_FRAMES[1]:-▌}}"
  POSTDISPLAY="$(_lmc_loading_indicator "$frame")"
  _LMC_CLEAR_MESSAGE_ON_NEXT_REDRAW=0
  _LMC_MESSAGE_VISIBLE=1
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

_lmc_reset_state() {
  emulate -L zsh

  local animation_pid="$_LMC_ANIMATION_PID"
  _LMC_ANIMATION_PID=-1
  if (( animation_pid > 0 )); then
    command kill "$animation_pid" 2>/dev/null
  fi

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
  if [[ -n "$_LMC_EVENT_FILE" ]]; then
    command rm -f -- "$_LMC_EVENT_FILE"
  fi

  _LMC_OUTPUT_FILE=""
  _LMC_ERROR_FILE=""
  _LMC_STATUS_FILE=""
  _LMC_EVENT_FILE=""
  _LMC_PREVIEW_BUFFER=""
  _LMC_EXPLAIN_BUFFER=""
}

_lmc_emit_loading_ticks() {
  emulate -L zsh

  local event_file="$1"
  local command_pid="$2"
  local frame_count="${#_LMC_LOADING_FRAMES[@]}"
  (( frame_count > 0 )) || return 0

  local frame_index=$(( frame_count > 1 ? 2 : 1 ))
  local frame

  while ! _lmc_has_exit_code && ! _lmc_command_finished "$command_pid"; do
    frame="${_LMC_LOADING_FRAMES[$frame_index]}"
    print -r -- $'tick\t'"$frame" > "$event_file" || break
    frame_index=$(( frame_index % frame_count + 1 ))
    sleep 0.09
  done
}

_lmc_apply_result() {
  emulate -L zsh

  local output="$1"

  if [[ "$output" == "# WARNING: destructive command"$'\n'* ]]; then
    BUFFER="${output#*$'\n'}"
    _LMC_MESSAGE_VISIBLE=1
    zle -M -- "WARNING: destructive command"
  else
    BUFFER="$output"
    _lmc_clear_message
  fi

  CURSOR=${#BUFFER}
}

_lmc_show_expand_explanation() {
  emulate -L zsh

  local explanation="$1"
  local display="$2"
  local warning="$3"
  local -a messages=()

  if [[ -n "$warning" && "$warning" != "none" ]]; then
    messages+=("WARNING: destructive command")
  fi

  case "$display" in
    both)
      POSTDISPLAY="$(_lmc_explain_indicator "$explanation")"
      [[ -n "$explanation" ]] && messages+=("$explanation")
      ;;
    inline)
      POSTDISPLAY="$(_lmc_explain_indicator "$explanation")"
      ;;
    message)
      POSTDISPLAY=""
      [[ -n "$explanation" ]] && messages+=("$explanation")
      ;;
    *)
      POSTDISPLAY=""
      ;;
  esac

  if [[ "$display" != "off" && -n "$explanation" ]]; then
    _LMC_EXPLAIN_BUFFER="$BUFFER"
  fi

  if (( ${#messages[@]} )); then
    _LMC_MESSAGE_VISIBLE=1
    zle -M -- "${(j: | :)messages}"
  else
    _lmc_clear_message
  fi
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
    _lmc_show_error_message "${error:-lmc expansion failed}"
  fi

  _lmc_finalize_widget_display
}

_lmc_finish_stream_success() {
  emulate -L zsh

  local warning="$1"
  local command="$2"
  local explanation="$3"
  local display="${4:-off}"
  local output="$command"
  if [[ "$warning" == "warning" ]]; then
    output="# WARNING: destructive command"$'\n'"$command"
  fi

  _lmc_reset_state
  _lmc_apply_result "$output"
  _lmc_show_expand_explanation "$explanation" "$display" "$warning"
  _lmc_finalize_widget_display
}

_lmc_finish_stream_error() {
  emulate -L zsh

  local message="$1"
  _lmc_reset_state
  BUFFER="$_LMC_ORIGINAL_BUFFER"
  CURSOR=${#BUFFER}
  _lmc_show_error_message "${message:-lmc expansion failed}"
  _lmc_finalize_widget_display
}

_lmc_cancel_stream() {
  emulate -L zsh

  (( _LMC_IN_FLIGHT )) || return 0

  local command_pid="$_LMC_COMMAND_PID"
  if (( command_pid > 0 )); then
    command kill "$command_pid" 2>/dev/null
  fi

  _lmc_reset_state
  _lmc_clear_message
  _lmc_invalidate_display
}

_lmc_event_field() {
  emulate -L zsh

  local key="$1"
  shift
  local field
  for field in "$@"; do
    if [[ "$field" == "${key}="* ]]; then
      print -r -- "${field#*=}"
      return 0
    fi
  done
  return 1
}

_lmc_line_pre_redraw() {
  emulate -L zsh

  if (( _LMC_CLEAR_MESSAGE_ON_NEXT_REDRAW )); then
    _lmc_clear_message
    _lmc_soft_redraw
    _lmc_run_previous_line_pre_redraw
    return 0
  fi

  case "${LASTWIDGET:-}" in
    lmc-expand-buffer|lmc-tab-expand-or-complete|lmc-handle-expand-event|lmc-line-pre-redraw)
      _lmc_run_previous_line_pre_redraw
      return 0
      ;;
  esac

  if (( _LMC_IN_FLIGHT )); then
    _lmc_cancel_stream
    _lmc_run_previous_line_pre_redraw
    return 0
  fi

  if [[ -n "$_LMC_EXPLAIN_BUFFER" && "$BUFFER" != "$_LMC_EXPLAIN_BUFFER" ]]; then
    _lmc_clear_explanation
    _lmc_soft_redraw
    _lmc_run_previous_line_pre_redraw
    return 0
  fi

  _lmc_run_previous_line_pre_redraw
}

_lmc_handle_expand_eof() {
  emulate -L zsh

  local error=""
  if [[ -n "$_LMC_ERROR_FILE" && -f "$_LMC_ERROR_FILE" ]]; then
    error="$(<"$_LMC_ERROR_FILE")"
  fi

  if _lmc_has_exit_code; then
    local exit_code
    exit_code="$(<"$_LMC_STATUS_FILE")"
    if [[ "$exit_code" == "0" ]]; then
      if [[ -n "$_LMC_OUTPUT_FILE" && -f "$_LMC_OUTPUT_FILE" ]]; then
        _lmc_finish_expand 0
      else
        _lmc_finish_stream_error "${error:-lmc expansion ended without a completion event}"
      fi
    else
      _lmc_finish_stream_error "${error:-lmc expansion failed}"
    fi
  else
    if [[ -n "$_LMC_OUTPUT_FILE" && -f "$_LMC_OUTPUT_FILE" ]] && _lmc_command_finished "$_LMC_COMMAND_PID"; then
      _lmc_finish_expand 0
    else
      _lmc_finish_stream_error "${error:-lmc expansion ended unexpectedly}"
    fi
  fi
}

_lmc_handle_expand_event() {
  emulate -L zsh

  local fd="$1"
  local line

  if ! IFS= read -r line <&"$fd"; then
    _lmc_handle_expand_eof
    return 0
  fi

  while :; do
    local -a _lmc_parts
    _lmc_parts=("${(@ps:\t:)line}")
    local event="${_lmc_parts[1]}"

    case "$event" in
      tick)
        _lmc_show_loading_state "${_lmc_parts[2]:-...}"
        _lmc_soft_redraw
        ;;
      preview)
        local preview
        preview="$(_lmc_event_field command "${_lmc_parts[@]:1}")"
        if [[ -n "$preview" ]]; then
          _LMC_PREVIEW_BUFFER="$preview"
        fi
        POSTDISPLAY=""
        zle -M ""
        _lmc_soft_redraw
        ;;
      done)
        if [[ "${_lmc_parts[2]:-}" == <-> ]]; then
          _lmc_finish_expand "${_lmc_parts[2]}"
          return 0
        fi

        local result_status
        result_status="$(_lmc_event_field status "${_lmc_parts[@]:1}")"
        if [[ "$result_status" == "ok" ]]; then
          local warning command explanation display
          warning="$(_lmc_event_field warning "${_lmc_parts[@]:1}")"
          command="$(_lmc_event_field command "${_lmc_parts[@]:1}")"
          explanation="$(_lmc_event_field explanation "${_lmc_parts[@]:1}")"
          display="$(_lmc_event_field display "${_lmc_parts[@]:1}")"
          _lmc_finish_stream_success \
            "${warning:-none}" \
            "$command" \
            "$explanation" \
            "${display:-off}"
        else
          local message
          message="$(_lmc_event_field message "${_lmc_parts[@]:1}")"
          _lmc_finish_stream_error "${message:-lmc expansion failed}"
        fi
        return 0
        ;;
    esac

    if ! IFS= read -r -t 0 line <&"$fd"; then
      return 0
    fi
  done
}

_lmc_expand_buffer() {
  emulate -L zsh

  if (( _LMC_IN_FLIGHT )); then
    zle -M -- "lmc expansion already in progress"
    _lmc_soft_redraw
    return 0
  fi

  local raw_input="$BUFFER"
  if [[ -z "$raw_input" ]]; then
    zle redisplay
    return 0
  fi

  if ! _lmc_has_provider_config; then
    zle -M -- "$(_lmc_missing_provider_message)"
    _lmc_soft_redraw
    return 0
  fi

  local input="$raw_input"
  if [[ "$input" == '?? '* ]]; then
    input="${input#\?\? }"
  fi

  _lmc_clear_explanation
  _LMC_ORIGINAL_BUFFER="$BUFFER"
  _LMC_ERROR_FILE="$(mktemp "${TMPDIR:-/tmp}/lmc-error.XXXXXX")" || {
    return 1
  }
  _LMC_STATUS_FILE="$(mktemp "${TMPDIR:-/tmp}/lmc-status.XXXXXX")" || {
    _LMC_ERROR_FILE=""
    return 1
  }
  _LMC_EVENT_FILE="$(mktemp -u "${TMPDIR:-/tmp}/lmc-events.XXXXXX")" || {
    command rm -f -- "$_LMC_ERROR_FILE" "$_LMC_STATUS_FILE"
    _LMC_ERROR_FILE=""
    _LMC_STATUS_FILE=""
    return 1
  }

  command rm -f -- "$_LMC_STATUS_FILE"
  command mkfifo "$_LMC_EVENT_FILE" || {
    command rm -f -- "$_LMC_ERROR_FILE" "$_LMC_STATUS_FILE"
    _LMC_ERROR_FILE=""
    _LMC_STATUS_FILE=""
    _LMC_EVENT_FILE=""
    return 1
  }
  exec {_LMC_EVENT_FD}<>"$_LMC_EVENT_FILE" || {
    command rm -f -- "$_LMC_EVENT_FILE" "$_LMC_ERROR_FILE" "$_LMC_STATUS_FILE"
    _LMC_ERROR_FILE=""
    _LMC_STATUS_FILE=""
    _LMC_EVENT_FILE=""
    return 1
  }

  (
    command lmc expand --stream-format widget --shell zsh -- "$input" >"$_LMC_EVENT_FILE" 2>"$_LMC_ERROR_FILE"
    print -r -- "$?" >|"$_LMC_STATUS_FILE"
  ) &!

  _LMC_COMMAND_PID=$!
  _LMC_IN_FLIGHT=1
  _lmc_emit_loading_ticks "$_LMC_EVENT_FILE" "$_LMC_COMMAND_PID" &!
  _LMC_ANIMATION_PID=$!
  _lmc_show_loading_state
  zle -F -w "$_LMC_EVENT_FD" lmc-handle-expand-event
  _lmc_soft_redraw
}

_lmc_tab_expand_or_complete() {
  emulate -L zsh

  if [[ "$BUFFER" == '?? '* ]]; then
    _lmc_expand_buffer
  else
    zle expand-or-complete
  fi
}

_lmc_capture_previous_line_pre_redraw
zle -N lmc-expand-buffer _lmc_expand_buffer
zle -N lmc-handle-expand-event _lmc_handle_expand_event
zle -N lmc-tab-expand-or-complete _lmc_tab_expand_or_complete
zle -N zle-line-pre-redraw _lmc_line_pre_redraw
bindkey '^[[Z' lmc-expand-buffer
bindkey '^I' lmc-tab-expand-or-complete
