# Mouse

| Gesture                  | Where                    | Effect                                                     |
| ------------------------ | ------------------------ | ---------------------------------------------------------- |
| click                    | panel / row              | focus the panel, select the row                            |
| click                    | tab title                | switch tab                                                 |
| click                    | hint bar entry, `[button]` | run that action                                          |
| click                    | scrollbar                | jump; drag to scroll                                       |
| double-click             | file                     | stage / unstage (directory: fold / unfold)                 |
| double-click             | branch / remote branch / tag | checkout                                               |
| double-click             | commit / reflog / stash  | open it in the main view                                   |
| double-click             | submodule / worktree     | open it                                                    |
| double-click             | diff line / hunk header  | stage / unstage that line (or the hunk)                    |
| double-click             | conflict side            | keep ours / theirs (marker line: both)                     |
| double-click             | blame / history line     | show that commit                                           |
| double-click             | `--stat` line (commit)   | jump to that file's diff                                  |
| click                    | pinned file header       | jump to the start of that file                             |
| hover                    | commit                   | tooltip: full hash, author / email, date, refs, parents, full message |
| right-click              | anywhere                 | actions menu of the item under the cursor                  |
| wheel                    | any panel                | scroll the panel under the cursor (shift: horizontal)      |
| drag                     | main view left border    | resize the left column                                     |
| drag                     | command log top border   | resize the command log                                     |
| drag                     | diff                     | select a range of lines (then `space` / `d` / `y`)         |
| drag                     | rebase todo row          | reorder commits                                            |
| click                    | commit popup             | place the cursor; toggle amend / no-verify / signoff       |
| click outside            | menu / help / message    | close it                                                   |

Copy (`y`) uses OSC 52, supported by most terminals (and tmux with
`set -g set-clipboard on`).
