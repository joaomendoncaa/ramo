```
╭ TUI PREVIEW ──────────────────────────────────────────────────────────────────╮
│                                                                               │
│   project6                                                                    │
│   project5                     <------  (a directory inside ~/Projects)       │
│   project4                                                                    │
│   project3 +20 -20               <----  (ewww, a dirty directory!)            │
│   project2                                                                    │
│   project1                                                                    │
│ * project12 +2000 -340        <-------  (open dirs are prefixed with a *)     │
│ │ ⑂ feat/whatever +20 -40      <------  (a worktree is a session as well)     │
│ │ │ ⠼ Agent running         <---------  (an agent is always inside some       │
│ │ └ ✓ Agent idle                         worktree or directory, in a pane     │
│ └ ⑂ fix/whatever +1980 -300              you can jump into)                   │
│ * project11                                                                   │
│ * SSH project10 +2003 -340       <----  (another directory but this one is    │
│ │ ⠧ Agent running in a directory         on a remote box with "SSH" as its    │
│ └ ⑂ feat/whatever +1903 -34              SSH alias 🤯)                        │
│ * project9 +2003 -340                                                         │
│ * project8 feat/whatever        <-----  (if root dirs are in non-master       │
│ * project7                               branches, ramo will show you)        │
│                                                                               │
│ ▸ █                     <-------------  (fuzzy search sessions)               │
│                                                                               │
│ : command mode                <-------  (press ":" to enter command mode)     │
╰───────────────────────────────────────────────────────────────────────────────╯
```

> [!CAUTION]
> It's a work in progress, missing key features, with a lot of bugs.

Everyone is overcomplicating agent orchestrators and multiplexers — we had it so good with all the local tmux session fuzzy finders — and so **Ramo** was born (twig in Portuguese, [pronounced *"Rah-moo"* 🕪](https://pt.bab.la/pron%C3%BAncia/portugues/ramo)). Gives you a TUI that you can `tmux display-popup` with, which adds **local and remote tmux session** navigation with **worktrees** and **agents** in mind. Worktrees are just an extra directory, and thus an extra session you can fly by. Agents are managed by you (I spawn them in random panes), but they are also listed as an entry. Visually, all these entries appear in a tree, where worktrees always relate to some "main project directory", and agents to one of those worktrees or directories. The cherry-on-top is that all of this applies to remote boxes behind SSH tunnels — they're just extra directories anyway.

**Contributions** are welcome for more agent harnesses, multiplexers, terminals, and platforms support (and obviously bug fixes). If you want to see Ramo on your favorite toolchain, send patches!

**Install** through `cargo install ramo`.

**Config** by `ramo config`, or look at the [example config here](./config).

**Help** with ~~800-328-8476~~ `ramo help`.
