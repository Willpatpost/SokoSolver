import type { Difficulty, PuzzleDefinition } from "../core/types.ts";

export const PUZZLES: readonly PuzzleDefinition[] = [
  {
    id: "ultra-tiny",
    title: "First Steps",
    difficulty: "tutorial",
    boxes: 1,
    hint: "Push the box down onto its goal.",
    rows: ["OOOOO", "O R O", "O A O", "O a O", "OOOOO"],
  },
  {
    id: "tiny",
    title: "Two's Company",
    difficulty: "tutorial",
    boxes: 2,
    hint: "Generic boxes go to S goals. Labeled boxes go to matching goals.",
    rows: ["OOOOOO", "O R  O", "O XO O", "OO A O", "OSa  O", "OOOOOO"],
  },
  {
    id: "tutorial-push",
    title: "One Push Wonder",
    difficulty: "tutorial",
    boxes: 1,
    hint: "Walk up and push the box right onto the goal.",
    rows: ["OOOOO", "O XSO", "O   O", "O R O", "OOOOO"],
  },
  {
    id: "tutorial-around",
    title: "Go Around",
    difficulty: "tutorial",
    boxes: 1,
    hint: "You can't pull boxes. Walk around to push from the other side.",
    rows: ["OOOOOOO", "OR    O", "OOOOX O", "O   S O", "OOOOOOO"],
  },
  {
    id: "beginner-three",
    title: "Three in a Row",
    difficulty: "beginner",
    boxes: 3,
    hint: "Order matters! Start with the box farthest from the goals.",
    rows: ["OOOOOOOO", "O R    O", "O XXXO O", "O SSSO O", "O      O", "OOOOOOOO"],
  },
  {
    id: "beginner-detour",
    title: "The Detour",
    difficulty: "beginner",
    boxes: 2,
    hint: "The direct path is blocked. Find the scenic route.",
    rows: ["OOOOOOOO", "OR     O", "OOOO X O", "OS   X O", "OS     O", "OOOOOOOO"],
  },
  {
    id: "beginner-typed-line",
    title: "Color Line",
    difficulty: "beginner",
    boxes: 3,
    hint: "Each box must reach its matching color. Plan the sequence!",
    rows: ["OOOOOOOOO", "Oc b a  O", "O       O", "O A B C O", "O   R   O", "OOOOOOOOO"],
  },
  {
    id: "box-5x5-a",
    title: "Tiny Teaser",
    difficulty: "beginner",
    boxes: 2,
    hint: "A deceptively simple 5x5. Think before you push!",
    rows: ["OOOOO", "OSX O", "O XRO", "O  SO", "OOOOO"],
  },
  {
    id: "medium",
    title: "Color Wheel",
    difficulty: "intermediate",
    boxes: 8,
    hint: "Each labeled box has a specific home. Plan the order carefully.",
    rows: ["OOOOOOO", "Oa   bO", "O AXB O", "O XRX O", "OSCXDSO", "OcS SdO", "OOOOOOO"],
  },
  {
    id: "garden-2",
    title: "Garden Path",
    difficulty: "intermediate",
    boxes: 2,
    hint: "Wind through the garden. Mind the hedges!",
    rows: [
      "OOOOOOOOOOO",
      "O    R    O",
      "O OOO OOO O",
      "O A     B O",
      "O OOO OOO O",
      "O  b   a  O",
      "O OO O OO O",
      "O         O",
      "OOOOOOOOOOO",
    ],
  },
  {
    id: "workshop-1",
    title: "Tool Shed",
    difficulty: "intermediate",
    boxes: 3,
    hint: "Small space, big challenge. Every move counts.",
    rows: ["OOOOOOO", "O   R O", "O OXO O", "O X   O", "OSX   O", "OS    O", "OS    O", "OOOOOOO"],
  },
  {
    id: "classic-1",
    title: "Original Spirit",
    difficulty: "intermediate",
    boxes: 3,
    hint: "Inspired by classic Sokoban. The side alcove is key.",
    rows: ["OOOOOOO", "O     O", "O OXO O", "O  X  O", "OO X OO", "O  R  O", "O SSS O", "OOOOOOO"],
  },
  {
    id: "theme-kitchen",
    title: "Kitchen Cleanup",
    difficulty: "intermediate",
    boxes: 3,
    hint: "Push the ingredients to the counter. Mind the kitchen island!",
    rows: [
      "OOOOOOOOO",
      "O R     O",
      "O  OOO  O",
      "O X O X O",
      "O  O    O",
      "O  O  X O",
      "O SSS   O",
      "OOOOOOOOO",
    ],
  },
  {
    id: "large",
    title: "The Warehouse",
    difficulty: "advanced",
    boxes: 6,
    hint: "Navigate the L-shaped corridor. Don't trap boxes against walls.",
    rows: [
      "OOOOOOOOOO",
      "OOOOOOOSSO",
      "OOOOO  abO",
      "OOOOO XSSO",
      "OOOOOO  OO",
      "OR     OOO",
      "OO A X X O",
      "OO BXO O O",
      "OO   O   O",
      "OOOOOOOOOO",
    ],
  },
  {
    id: "adv-gallery",
    title: "The Gallery",
    difficulty: "advanced",
    boxes: 4,
    hint: "Boxes line the gallery walls. Slide them to the exhibition spots.",
    rows: [
      "OOOOOOOOOO",
      "O R      O",
      "O OOOOOO O",
      "O O    O O",
      "O X SS X O",
      "O O    O O",
      "O OXOOXO O",
      "O        O",
      "O   SS   O",
      "OOOOOOOOOO",
    ],
  },
  {
    id: "theme-parking",
    title: "Parking Lot",
    difficulty: "advanced",
    boxes: 5,
    hint: "Park each car in its assigned spot. Don't block the exit!",
    rows: [
      "OOOOOOOOOOO",
      "O   R     O",
      "O A B C   O",
      "O  OOOOO  O",
      "O    X  X O",
      "O  OOOOO  O",
      "O a b c   O",
      "O      SSOO",
      "OOOOOOOOOOO",
    ],
  },
  {
    id: "expert-maze",
    title: "The Maze",
    difficulty: "expert",
    boxes: 5,
    hint: "A winding maze with boxes at dead ends. Free them carefully.",
    rows: [
      "OOOOOOOOOOOO",
      "O R  O     O",
      "OOO  O OOO O",
      "O X  O O S O",
      "O OO   O   O",
      "O O  OOOO  O",
      "O   XO  X  O",
      "OOOO OS    O",
      "O  X    OO O",
      "O SSS X    O",
      "OOOOOOOOOOOO",
    ],
  },
  {
    id: "huge",
    title: "Grand Hall",
    difficulty: "expert",
    boxes: 17,
    hint: "This symmetric puzzle has mirrored rooms. Solve the outer wings first.",
    rows: [
      "OOOOOOOOOOOOOOO",
      "OaSS   S   SSbO",
      "OSCS  OOO  SDSO",
      "OX X  OOO  X XO",
      "O     OOO     O",
      "OOOO   X   OOOO",
      "O      O      O",
      "O G hOOOOOH g O",
      "O      O      O",
      "OOO         OOO",
      "OOO   X X   OOO",
      "OOOOOOOROOOOOOO",
      "O B X X X X A O",
      "O Sc       dS O",
      "OOOOOOOOOOOOOOO",
    ],
  },
];

const DIFFICULTY_ORDER: readonly Difficulty[] = [
  "tutorial",
  "beginner",
  "intermediate",
  "advanced",
  "expert",
  "master",
];

const difficultyRank = new Map(
  DIFFICULTY_ORDER.map((d, i) => [d, i]),
);

export function getOrderedPuzzles(): readonly PuzzleDefinition[] {
  return [...PUZZLES].sort(
    (a, b) =>
      (difficultyRank.get(a.difficulty) ?? 99) -
      (difficultyRank.get(b.difficulty) ?? 99),
  );
}

export function getPuzzleById(id: string): PuzzleDefinition | undefined {
  return PUZZLES.find((p) => p.id === id);
}

export function getPuzzlesByDifficulty(
  difficulty: Difficulty,
): readonly PuzzleDefinition[] {
  return PUZZLES.filter((p) => p.difficulty === difficulty);
}
