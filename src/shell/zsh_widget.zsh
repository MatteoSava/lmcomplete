_lmc_expand_buffer() {
  emulate -L zsh

  local raw_input="$BUFFER"
  if [[ -z "$raw_input" ]]; then
    zle redisplay
    return 0
  fi

  local input="$raw_input"
  if [[ "$input" == '?? '* ]]; then
    input="${input#\?\? }"
  fi

  local output
  output="$(lmc expand --shell zsh -- "$input")" || return $?
  BUFFER="$output"
  CURSOR=${#BUFFER}
  zle redisplay
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
zle -N lmc-tab-expand-or-complete _lmc_tab_expand_or_complete

bindkey '^[[Z' lmc-expand-buffer
bindkey '^I' lmc-tab-expand-or-complete
