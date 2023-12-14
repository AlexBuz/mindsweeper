# Mindsweeper — a principled take on minesweeper

To play, visit https://alexbuz.github.io/minesweeper/. Once the page loads, no further internet connection is required.

## Background

Traditional minesweeper is a game of logical deduction—until it's not. Sometimes, you end up in a situation where there is not enough information to find a tile that is definitely safe to reveal. In such cases, guessing is required to proceed, and that often leads to the loss of an otherwise smooth-sailing game. However, there's no reason it has to be that way. It's merely a consequence of the random manner in which mines are typically arranged at the start of the game. Some mine arrangements happen to necessitate guessing, while others do not. This is a matter of luck, and it's not a particularly fun aspect of a game that is otherwise about logic.

Eliminating the need for guesswork, then, is a matter of modifying the mine arrangement algorithm. Rather than simply placing each mine under a random tile, it should instead consider each mine arrangement as a whole, choosing a random mine arrangement from the set of mine arrangements that would allow a perfect logician to win without guessing. Ideally, it should sample uniformly from that set of mine arrangements, ensuring that every such arrangement is equally likely to be chosen. That is precisely what mindsweeper is designed to do, and it accomplishes this within a matter of milliseconds after you make your first click.

## Features

1. Guessing is *never* necessary
    - There's no need to toggle a setting. Mindsweeper is a game of pure skill, always.
2. Guess punishment
    - Since guessing is already unnecessary, it's only natural to take the idea of "no guessing" a step further and forbid guessing entirely. This feature, enabled by default, effectively rids the game of all remaining luck aspects. If you click on a tile that *can* be a mine, then it *will* be a mine, guaranteed.
3. Unrestricted first click
    - Mindsweeper does not obligate you to click a particular tile to start the game. The mine arrangement algorithm works on demand, and is fast enough to avoid introducing any delay.
4. Uniform sampling
    - All mine arrangements are viable, except those that necessitate guessing. If a particular mine arrangement is solvable by a perfect logician without guessing, then it's just as likely to be picked by the algorithm as every other viable arrangement.
5. Post-mortem analysis
    - If you reveal a mine and lose the game, you'll get feedback that helps you improve. You get to see which flags you misplaced (if any), as well as which tiles you could (and could not) have safely revealed. Tiles are color-coded to show this information at a glance, and you can also hover over any tile to see this explained in words.
6. High performance
    - Mindsweeper is written in Rust and compiles to WASM. When you make your first click, the mine arrangement algorithm generally finishes running before you even release the mouse button, so there is no first-click delay.
7. Completely offline
    - Mindsweeper does not depend on a server. All of the code runs locally in your browser.

## Building from source

Install [Trunk](https://trunkrs.dev/), and then run `trunk build` in the project directory:

```sh
git clone https://github.com/alexbuz/mindsweeper.git
cd mindsweeper
trunk build
```

The built files will be placed in the `dist`, the contents of which is served by GitHub Pages when you visit https://alexbuz.github.io/minesweeper/ to play the game.