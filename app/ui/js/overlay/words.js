// Word suggestions for the Orbit rail: common English words and gaming terms, most useful first
// (the first match for a prefix is what Y takes). Hand-picked; lower case.

export const WORDS = `
the to and a of i is in it you that how what do for on can with my this be are get where why which when
who does have not is it best way beat find use make need there next boss level game item weapon build
combo map quest mission move moves attack defense defence skill skills upgrade unlock unlocked craft crafting
farm farming fast faster easy easiest hard hardest early late good better strongest damage health armor
armour shield sword gun guns ammo spell spells magic mana stamina character characters class classes
should does did will would could get got go going gone go out back up down left right jump run dodge
block parry counter special finisher fatality brutality spear teleport uppercut kick punch grab throw
juggle cancel frame frames input inputs button buttons controller trigger stick
key keys door puzzle puzzles solve solution secret secrets hidden chest chests loot drop drops rare
legendary epic common gold coins money xp exp experience points rank ranked match matches win wins
lose loss team teammate enemy enemies npc npcs friend friends party squad lobby server servers lag ping
fps setting settings graphics sensitivity aim aiming recoil reload sprint crouch prone scope sniper
shotgun rifle pistol smg grenade bomb trap traps heal healing potion potions buff buffs nerf nerfed meta
tier list loadout perk perks ability abilities ultimate cooldown cooldowns passive active stats stat
strength dexterity intelligence vitality luck speed critical crit chance resistance fire ice poison
lightning dark light holy blood boss fight fights phase phases weakness weak spot pattern patterns
walkthrough guide tips tip trick tricks glitch bug exploit cheat cheats code codes achievement
achievements trophy trophies chapter act area zone region world dungeon raid arena castle cave forest
village city tower temple shop merchant vendor blacksmith trader save checkpoint respawn spawn location
warden water wall walk wave waves wait want was way warrior wizard witch werewolf wolf dragon demon
dragon ghost goblin golem knight king queen prince princess lord master mage monk ninja samurai
scorpion sub zero raiden liu kang kitana mileena sonya johnny cage jax kano kung lao shao kahn
the then them they their there these those than that this thing things think time times today
about after again all also always am an any around as at away because been before being between
both but by came come could day did different each end even every few first from give great had has
he her here him his home if into just keep know last like little long look made many may me more
most much must name never new no now number off old one only or other our over own part people
place put read really right said same say see she show side small so some something sound still
such take tell than thank thanks through too try turn two under until upon us very want watch well
went were while white whole without word words work world write year years yes yet young your
change check close open start stop pause play played player players playing restart quit leave join
invite queue mode modes story campaign multiplayer online offline coop co-op solo duo trio
explain mean means meaning name names cost costs price worth trade sell buy
`.trim().split(/\s+/);

// Unique, keeping the first (most useful) position.
export const RANKED = [...new Set(WORDS)];
