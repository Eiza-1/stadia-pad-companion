// Gemini Live captions (pure): what the player said (one dim line, their latest words) and what Gemini
// is saying (the newest words bright, earlier words of the same turn dimmed). A new turn starts clean.

const BRIGHT = 12; // words of Gemini's turn shown bright
const OLDER = 20; // earlier words kept, dimmed
const YOU = 10; // words of the player's line

const lastWords = (t, n) => t.trim().split(/\s+/).filter(Boolean).slice(-n).join(" ");

export function createCaptions() {
  let you = "";
  let gem = "";
  let done = false; // Gemini's turn finished (or was interrupted): the next words start a new one

  return {
    you(piece) {
      if (done) {
        you = "";
        gem = "";
        done = false;
      }
      you += piece;
    },
    gemini(piece) {
      if (done) {
        gem = "";
        done = false;
      }
      gem += piece;
    },
    turnDone() {
      done = true;
    },
    interrupted() {
      done = true;
    },
    view() {
      const words = gem.trim().split(/\s+/).filter(Boolean);
      const current = words.slice(-BRIGHT).join(" ");
      const older = words.slice(Math.max(0, words.length - BRIGHT - OLDER), Math.max(0, words.length - BRIGHT)).join(" ");
      return { you: lastWords(you, YOU), current, older };
    },
  };
}
