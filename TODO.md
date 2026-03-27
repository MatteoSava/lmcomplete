# Beta 1

[ ] - context engineering what to pass and when
[ ] - auto update how?
[ ] - how to putt the info in grey are we using tool call are we streaming with shift tab?
[ ] - consider restoring LLM destructive hint in TTY prompt (`build_tty_system_prompt`) as a signal to improve streaming UX — the Rust safety layer already handles it, but the LLM hint could let the TTY renderer show warnings earlier during streaming (before `finalize_expand_command` runs post-stream)
