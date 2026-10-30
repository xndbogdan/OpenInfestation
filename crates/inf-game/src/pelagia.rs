//! Pelagia, the prototype world (world 1), with the rework
//! on ([the plan](../../../docs/rt-plans/prototype-world.md)). The
//! Command Centre (world 18) keeps its retail positions and facings, but
//! gate 81 still wakes: a scientist the player drops by it walks to it,
//! the port sends it retail's message 0x15 and gives it world 1. World 1
//! has only the one gate (placement 73), so there is no second Pelagia
//! gate to trade places with. World 1's own
//! gate (placement 73) leads back to the Command Centre's
//! first start with no scientist asked for, on the dead gate's site, the
//! ACAM coming out of it; its factories stock the
//! hovercraft and the headlights. Retail's data stays as it is; everything
//! added lives here, behind
//! [`PortFixes::prototype_world`](crate::frontend_settings::PortFixes).

use inf_formats::{
    BriefingCamera, BriefingModel, BriefingPayload, BriefingProgram, BriefingRecord, ByteSpan,
};

mod ground;

use crate::{
    object_runtime::{
        construct_world_object, BaseContext, ObjectArena, ObjectConstructionOutcome,
        ObjectConstructionRequest, ObjectId,
    },
    terrain_runtime::TerrainRuntime,
    Installation, ResidencyRequest, ResidentRuntime,
};

/// The prototype world, the campaign's selection 1.
pub const WORLD: u32 = 1;
/// The Command Centre (Level 1) and its dead start gate. The Command
/// Centre keeps its retail positions and facings: the port never moves
/// its gates, but gate 81 still wakes for a scientist.
pub const COMMAND_CENTRE: u32 = 18;
pub const DEAD_GATE: usize = 81;
/// World 1's own gate, retail's way to Terra Nova; the port's way back to
/// the Command Centre's first start.
pub const RETURN_GATE: usize = 73;
/// The hint trigger standing on the return gate, asking for a scientist.
pub const RETURN_HINT: usize = 44;
/// With the rework, the return gate and world 1's dead gate (the
/// `gatedead` furniture record, H8 286, the gate's own death model) trade
/// places (2026-10-25): the working gate stands on the dead
/// one's even cell on the east island, which the start's land reaches,
/// and the dead one where the working gate stood, on the next even cell.
pub const GATE_SITE: [u16; 2] = [50944, 26880];
pub const DEAD_GATE_SITE: [u16; 2] = [27904, 18944];
/// The dead gate's furniture lookup and its state at its new cell: its
/// turn (`state >> 4`, in 4096ths) about the return gate's authored
/// 23665.
pub const DEAD_GATE_LOOKUP: u8 = 97;
pub const DEAD_GATE_STATE: u8 = 0x60;
/// With the rework the ACAM comes out of the return gate (2026-10-25):
/// 450 behind it, where retail's gate starts put it
/// (`0048C5AC`, along its basis' third row), facing away from it.
pub const SPAWN: [u16; 2] = [50600, 27169];
pub const SPAWN_HEADING: i16 = 310;
/// The factory beside world 1's start, holding the hovercraft.
pub const START_FACTORY: usize = 2;
/// The factory nearest gate 73, holding Headlights beside its grenades.
pub const GATE_FACTORY: usize = 17;
/// Hovercraft mode and Headlights, the egg's products.
pub const HOVERCRAFT: u16 = 0xa7;
pub const HEADLIGHTS: u16 = 0x14;
/// What the gate factory keeps holding.
pub const STANDARD_GRENADES: u16 = 0x25;
/// The world's name, the same word in all six languages.
pub const NAME: &[u8] = b"Pelagia";

/// The scientist's report as gate 81 wakes, on the player's message line,
/// in `language` (retail's six, English past them).
pub fn report(language: u32) -> &'static [u8] {
    const REPORTS: [&[u8]; 6] = [
        b"Commander, I have successfully repaired this warpgate.",
        b"Commandant, j'ai r\xe9par\xe9 avec succ\xe8s ce portail intersid\xe9ral.",
        b"Commander, ich habe dieses Warpgate erfolgreich repariert.",
        b"Comandante, ho riparato con successo questo portale.",
        b"Comandante, he reparado con \xe9xito este portal de teletransporte.",
        b"Commandant, ik heb deze warppoort succesvol gerepareerd.",
    ];
    REPORTS
        .get(language as usize)
        .copied()
        .unwrap_or(REPORTS[0])
}

/// What each of Pelagia's colonists says as it boards the player's craft
/// ([`colonist_line`]), in retail's six languages' order: together they
/// tell the colony's story in play, a line a scientist.
const COLONIST_LINES: [[&[u8]; 6]; 6] = [
    [
        b"The others left on the last transport, Commander. I stayed to keep the factories running.",
        b"Les autres sont partis par le dernier transport, Commandant. Je suis rest\xe9 pour faire tourner les usines.",
        b"Die anderen sind mit dem letzten Transport abgereist, Commander. Ich blieb, damit die Fabriken laufen.",
        b"Gli altri sono partiti con l'ultimo trasporto, Comandante. Io sono rimasto a mandare avanti gli stabilimenti.",
        b"Los dem\xe1s se fueron en el \xfaltimo transporte, comandante. Yo me qued\xe9 para mantener las f\xe1bricas.",
        b"De anderen zijn met het laatste transport vertrokken, commandant. Ik bleef voor de fabrieken.",
    ],
    [
        b"We heard a warpgate hum for the first time in years. We thought the routes had come back.",
        b"Nous avons entendu un portail bourdonner pour la premi\xe8re fois depuis des ann\xe9es. Nous pensions les routes revenues.",
        b"Wir h\xf6rten zum ersten Mal seit Jahren ein Warpgate summen. Wir dachten, die Routen w\xe4ren zur\xfcck.",
        b"Abbiamo sentito ronzare un portale per la prima volta da anni. Pensavamo che le rotte fossero tornate.",
        b"O\xedmos zumbar un portal por primera vez en a\xf1os. Cre\xedmos que las rutas hab\xedan vuelto.",
        b"We hoorden voor het eerst in jaren een warppoort zoemen. We dachten dat de routes terug waren.",
    ],
    [
        b"Mind the creatures on the far islands, Commander. They have never liked visitors.",
        b"M\xe9fiez-vous des cr\xe9atures des \xeeles lointaines, Commandant. Elles n'ont jamais aim\xe9 les visiteurs.",
        b"H\xfcten Sie sich vor den Kreaturen auf den fernen Inseln, Commander. Besucher mochten sie nie.",
        b"Attento alle creature delle isole lontane, Comandante. Non hanno mai amato i visitatori.",
        b"Cuidado con las criaturas de las islas lejanas, comandante. Nunca les han gustado las visitas.",
        b"Pas op voor de wezens op de verre eilanden, commandant. Ze hebben nooit van bezoek gehouden.",
    ],
    [
        b"There are crystals out on the islets. None of us has had a hovercraft to reach them.",
        b"Il y a des cristaux sur les \xeelots du large. Aucun de nous n'avait d'a\xe9roglisseur pour les atteindre.",
        b"Drau\xdfen auf den Inselchen liegen Kristalle. Keiner von uns hatte ein Hovercraft, um hinzukommen.",
        b"Ci sono cristalli sugli isolotti. Nessuno di noi ha mai avuto un hovercraft per raggiungerli.",
        b"Hay cristales en los islotes. Ninguno de nosotros ha tenido un hovercraft para llegar a ellos.",
        b"Er liggen kristallen op de eilandjes. Geen van ons had een hovercraft om erbij te komen.",
    ],
    [
        b"Something sank in the lagoon long ago. On calm days you can see it glowing under the water.",
        b"Quelque chose a sombr\xe9 dans le lagon il y a longtemps. Par temps calme, on le voit briller sous l'eau.",
        b"Vor langer Zeit ist etwas in der Lagune versunken. An ruhigen Tagen sieht man es unter Wasser leuchten.",
        b"Qualcosa \xe8 affondato nella laguna tanto tempo fa. Nei giorni calmi lo si vede brillare sott'acqua.",
        b"Algo se hundi\xf3 en la laguna hace mucho. En los d\xedas de calma se lo ve brillar bajo el agua.",
        b"Lang geleden is er iets in de lagune gezonken. Op kalme dagen zie je het onder water gloeien.",
    ],
    [
        b"Tell the Command Centre that Pelagia is still here, Commander. Someone should remember us.",
        b"Dites au Centre de contr\xf4le que Pelagia existe toujours, Commandant. Quelqu'un doit se souvenir de nous.",
        b"Sagen Sie dem Kommandozentrum, dass es Pelagia noch gibt, Commander. Jemand sollte sich erinnern.",
        b"Di' al Centro di comando che Pelagia esiste ancora, Comandante. Qualcuno deve ricordarsi di noi.",
        b"Dile al Centro de Mando que Pelagia sigue aqu\xed, comandante. Alguien deber\xeda acordarse de nosotros.",
        b"Zeg tegen het Commandocentrum dat Pelagia er nog is, commandant. Iemand moet ons onthouden.",
    ],
];

/// The story line `line` of Pelagia's colonists in `language` (retail's
/// six, English past them), past the last wrapping round.
pub fn colonist_line(line: u8, language: u32) -> &'static [u8] {
    let lines = COLONIST_LINES[usize::from(line) % COLONIST_LINES.len()];
    lines.get(language as usize).copied().unwrap_or(lines[0])
}

/// H0 slots the port owns on world 1: the briefing's title, its lines, its
/// one objective, and the briefing header's world name. Slot 0xE1 keeps
/// "Completed" in retail's data; only the world's own draw says Pelagia.
pub const TITLE: i32 = 874;
pub const FIRST_LINE: i32 = 875;
pub const LAST_LINE: i32 = 883;
pub const OBJECTIVE: i32 = 896;
pub const HEADER_NAME: i32 = 0xe1;

/// The port's H0 text for world 1 in `language` (retail's six, English past
/// them): the name wherever world 1 is named, the briefing's lines and its
/// objectives, as the rework (`rework`) or retail's world 1 has it. None
/// keeps retail's bytes; the draw falls back to them for every slot and
/// world this does not own.
pub(crate) fn text(world: u32, language: u8, slot: i32, rework: bool) -> Option<&'static [u8]> {
    if world != WORLD {
        return None;
    }
    if matches!(slot, TITLE | HEADER_NAME) {
        return Some(NAME);
    }
    let language = if language < 6 { language as usize } else { 0 };
    let own = if rework {
        REWORK_LINES
    } else {
        RETAIL_GATE_LINES
    };
    SHARED_LINES
        .iter()
        .chain(own)
        .find(|(owned, _)| *owned == slot)
        .map(|(_, lines)| lines[language])
}

/// The briefing's lines either way, in retail's six languages' order.
const SHARED_LINES: &[(i32, [&[u8]; 6])] = &[
    (
        875,
        [
            b"This is Pelagia, an old water world on the edge of the outer colonies. It was settled nearly two hundred years before the alien invasion, in the early days of the warp routes.",
            b"Voici Pelagia, un ancien monde aquatique aux confins des colonies ext\xe9rieures. Il fut colonis\xe9 pr\xe8s de deux cents ans avant l'invasion extraterrestre, aux premiers temps des voies de transfert.",
            b"Dies ist Pelagia, eine alte Wasserwelt am Rand der \xe4u\xdferen Kolonien. Sie wurde fast zweihundert Jahre vor der Invasion der Au\xdferirdischen besiedelt, in den Anf\xe4ngen der Warprouten.",
            b"Questo \xe8 Pelagia, un antico pianeta oceanico ai margini delle colonie esterne. Fu colonizzato quasi duecento anni prima dell'invasione aliena, agli albori delle rotte spazio-temporali.",
            b"Este es Pelagia, un antiguo mundo acu\xe1tico en el l\xedmite de las colonias exteriores. Fue colonizado casi doscientos a\xf1os antes de la invasi\xf3n alien\xedgena, en los primeros tiempos de las rutas de teletransporte.",
            b"Dit is Pelagia, een oude waterwereld aan de rand van de kolonies. Ze werd bijna tweehonderd jaar voor de alien-invasie gekoloniseerd, in de begintijd van de warproutes.",
        ],
    ),
    (
        876,
        [
            b"Its settlers built their homes on its few islands and lived from the sea. When the warp network grew and its routes moved on, most of them left.",
            b"Ses colons ont b\xe2ti leurs maisons sur ses quelques \xeeles et vivaient de la mer. Quand le r\xe9seau de transfert s'est \xe9tendu et que ses routes se sont d\xe9plac\xe9es, la plupart sont partis.",
            b"Ihre Siedler bauten ihre H\xe4user auf den wenigen Inseln und lebten vom Meer. Als das Warpnetz wuchs und sich seine Routen verlagerten, zogen die meisten fort.",
            b"I suoi coloni costruirono le loro case sulle poche isole e vivevano del mare. Quando la rete spazio-temporale si estese e le sue rotte cambiarono, quasi tutti se ne andarono.",
            b"Sus colonos construyeron sus hogares en sus pocas islas y viv\xedan del mar. Cuando la red de teletransporte creci\xf3 y sus rutas cambiaron, la mayor\xeda se march\xf3.",
            b"De kolonisten bouwden hun huizen op de paar eilanden en leefden van de zee. Toen het warpnetwerk groeide en de routes verschoven, vertrokken de meesten.",
        ],
    ),
    (
        877,
        [
            b"Pelagia offers no strategic advantage, and the alien fleet has passed it by. Only a few of their creatures have been seen on the islands.",
            b"Pelagia ne pr\xe9sente aucun int\xe9r\xeat strat\xe9gique, et la flotte extraterrestre l'a ignor\xe9. Seules quelques-unes de leurs cr\xe9atures ont \xe9t\xe9 aper\xe7ues sur les \xeeles.",
            b"Pelagia bietet keinen strategischen Vorteil, und die Flotte der Au\xdferirdischen hat sie \xfcbergangen. Nur einige ihrer Kreaturen wurden auf den Inseln gesichtet.",
            b"Pelagia non offre alcun vantaggio strategico e la flotta aliena lo ha trascurato. Sulle isole sono state avvistate solo alcune delle loro creature.",
            b"Pelagia no ofrece ninguna ventaja estrat\xe9gica y la flota alien\xedgena lo ha pasado por alto. Solo se han visto algunas de sus criaturas en las islas.",
            b"Pelagia levert geen strategisch voordeel op en de alienvloot is haar voorbijgegaan. Alleen een paar van hun wezens zijn op de eilanden gezien.",
        ],
    ),
    (
        878,
        [
            b"We were not expecting you here. The Command Centre's warpgate had been dead for years, and nobody knew where it led. Your scientist has reopened a forgotten route.",
            b"Nous ne vous attendions pas ici. Le portail intersid\xe9ral du Centre de contr\xf4le \xe9tait hors service depuis des ann\xe9es, et personne ne savait o\xf9 il menait. Votre savant a rouvert une route oubli\xe9e.",
            b"Wir haben Sie hier nicht erwartet. Das Warpgate im Kommandozentrum war seit Jahren au\xdfer Betrieb, und niemand wusste, wohin es f\xfchrt. Ihr Wissenschaftler hat eine vergessene Route wieder ge\xf6ffnet.",
            b"Non ti aspettavamo qui. Il portale del Centro di comando era fuori uso da anni e nessuno sapeva dove conducesse. Il tuo scienziato ha riaperto una rotta dimenticata.",
            b"No te esper\xe1bamos aqu\xed. El portal de teletransporte del Centro de Mando llevaba a\xf1os sin funcionar y nadie sab\xeda ad\xf3nde llevaba. Tu cient\xedfico ha reabierto una ruta olvidada.",
            b"We hadden je hier niet verwacht. De warppoort in het Commandocentrum werkte al jaren niet meer en niemand wist waar hij naartoe leidde. Je wetenschapper heeft een vergeten route heropend.",
        ],
    ),
    (
        883,
        [
            b"Enjoy the sunset, commander. Transmission ends.",
            b"Profitez du coucher de soleil, Commandant. Transmission termin\xe9e.",
            b"Genie\xdfen Sie den Sonnenuntergang, Commander. \xdcbertragung beendet.",
            b"Goditi il tramonto, Comandante. Fine della trasmissione.",
            b"Disfruta de la puesta de sol, comandante. Fin de la transmisi\xf3n.",
            b"Geniet van de zonsondergang, commandant. Einde bericht.",
        ],
    ),
];

/// The rework's colony ([`PortFixes::pelagia_rework`](crate::frontend_settings::PortFixes)):
/// its gate leads home, its factories stock the hovercraft and the
/// headlights, and its four objectives.
const REWORK_LINES: &[(i32, [&[u8]; 6])] = &[
    (
        879,
        [
            b"Your only objective is to get back to the Command Centre and finish your training.",
            b"Votre seul objectif : regagner le Centre de contr\xf4le et terminer votre entra\xeenement.",
            b"Ihr einziges Ziel ist es, zum Kommandozentrum zur\xfcckzukehren und Ihre Ausbildung abzuschlie\xdfen.",
            b"Il tuo unico obiettivo \xe8 tornare al Centro di comando e completare l'addestramento.",
            b"Tu \xfanico objetivo es volver al Centro de Mando y terminar tu entrenamiento.",
            b"Je enige doel is terugkeren naar het Commandocentrum en je training afronden.",
        ],
    ),
    (
        880,
        [
            b"The colony's old warpgate, situated here, still works and will take you back.",
            b"Le vieux portail intersid\xe9ral de la colonie, situ\xe9 ici, fonctionne encore : il vous ram\xe8nera.",
            b"Das alte Warpgate der Kolonie, das sich hier befindet, funktioniert noch und bringt Sie zur\xfcck.",
            b"Il vecchio portale spazio-temporale della colonia, situato qui, funziona ancora e ti riporter\xe0 indietro.",
            b"El viejo portal de teletransporte de la colonia, situado aqu\xed, a\xfan funciona y te llevar\xe1 de vuelta.",
            b"De oude warppoort van de kolonie, die zich hier bevindt, werkt nog en brengt je terug.",
        ],
    ),
    (
        881,
        [
            b"The colony's factories still work too. This one can give your ACAM hovercraft mode, to carry you across the open sea to the outer islets.",
            b"Les usines de la colonie fonctionnent elles aussi. Celle-ci peut doter votre VCM du mode a\xe9roglisseur, pour traverser la haute mer jusqu'aux \xeelots du large.",
            b"Auch die Fabriken der Kolonie arbeiten noch. Diese hier kann Ihren ACAM mit dem Hovercraft-Modus ausr\xfcsten, mit dem Sie \xfcber das offene Meer zu den \xe4u\xdferen Inselchen gelangen.",
            b"Anche gli stabilimenti della colonia funzionano ancora. Questo pu\xf2 dotare il tuo ACAM della modalit\xe0 hovercraft, per attraversare il mare aperto fino agli isolotti pi\xf9 lontani.",
            b"Las f\xe1bricas de la colonia tambi\xe9n funcionan. Esta puede dotar a tu MACA del modo hovercraft, para cruzar el mar abierto hasta los islotes exteriores.",
            b"Ook de fabrieken van de kolonie werken nog. Deze kan je GGAE de hovercraft-stand geven, waarmee je over open zee naar de buitenste eilandjes kunt varen.",
        ],
    ),
    (
        882,
        [
            b"The factory here, on the west island, can fit your ACAM with headlights. You may want them, as night falls on Pelagia.",
            b"L'usine situ\xe9e ici, sur l'\xeele de l'ouest, peut \xe9quiper votre VCM de feux avant. Ils pourraient vous \xeatre utiles, car la nuit tombe sur Pelagia.",
            b"Die Fabrik hier, auf der Westinsel, kann Ihren ACAM mit Scheinwerfern ausr\xfcsten. Sie werden sie brauchen, denn \xfcber Pelagia bricht die Nacht herein.",
            b"Lo stabilimento situato qui, sull'isola occidentale, pu\xf2 dotare il tuo ACAM di fanali anteriori. Potrebbero esserti utili, perch\xe9 su Pelagia cala la notte.",
            b"La f\xe1brica situada aqu\xed, en la isla del oeste, puede equipar tu MACA con luces largas. Puede que las necesites, porque en Pelagia cae la noche.",
            b"De fabriek hier, op het westelijke eiland, kan je GGAE van koplampen voorzien. Die kun je goed gebruiken, want op Pelagia valt de nacht.",
        ],
    ),
    (
        896,
        [
            b"1 Resupply with crystals, ammunition and any upgrades you find",
            b"1 Faire le plein de cristaux, de munitions et d'\xe9quipements",
            b"1 Versorgen Sie sich mit Kristallen, Munition und Upgrades",
            b"1 Rifornirsi di cristalli, munizioni e potenziamenti",
            b"1 Reabastecerte de cristales, munici\xf3n y mejoras",
            b"1 Vul je kristallen, munitie en opwaarderingen aan",
        ],
    ),
    (
        897,
        [
            b"2 Warp back to the Command Centre and finish your training",
            b"2 Regagner le Centre de contr\xf4le et terminer votre entra\xeenement",
            b"2 Warpen Sie zur\xfcck zum Kommandozentrum und beenden Sie Ihre Ausbildung",
            b"2 Tornare al Centro di comando e completare l'addestramento",
            b"2 Volver al Centro de Mando y terminar tu entrenamiento",
            b"2 Warp terug naar het Commandocentrum en rond je training af",
        ],
    ),
    (
        898,
        [
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
        ],
    ),
    (
        899,
        [
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
        ],
    ),
];

/// Retail's world 1, the rework off: its gate needs no scientist and
/// leads to Terra Nova, its factories hold nothing
/// new, and its two objectives (the other rows stay empty, as retail's
/// shorter lists leave theirs).
const RETAIL_GATE_LINES: &[(i32, [&[u8]; 6])] = &[
    (
        879,
        [
            b"Your only objective is to get off Pelagia. Its warpgate does not lead back to the Command Centre, so your training ends here.",
            b"Votre seul objectif : quitter Pelagia. Son portail ne ram\xe8ne pas au Centre de contr\xf4le : votre entra\xeenement s'arr\xeate donc ici.",
            b"Ihr einziges Ziel ist es, Pelagia zu verlassen. Das Warpgate hier f\xfchrt nicht zum Kommandozentrum zur\xfcck, Ihre Ausbildung endet also hier.",
            b"Il tuo unico obiettivo \xe8 lasciare Pelagia. Il suo portale non riporta al Centro di comando: il tuo addestramento finisce qui.",
            b"Tu \xfanico objetivo es salir de Pelagia. Su portal no lleva de vuelta al Centro de Mando, as\xed que tu entrenamiento termina aqu\xed.",
            b"Je enige doel is Pelagia verlaten. De warppoort hier leidt niet terug naar het Commandocentrum, dus je training eindigt hier.",
        ],
    ),
    (
        880,
        [
            b"The colony's old warpgate, situated here, still works, and it leads to the front line on Terra Nova.",
            b"Le vieux portail intersid\xe9ral de la colonie, situ\xe9 ici, fonctionne encore, et il m\xe8ne au front, sur Terra Nova.",
            b"Das alte Warpgate der Kolonie, das sich hier befindet, funktioniert noch und f\xfchrt an die Front nach Terra Nova.",
            b"Il vecchio portale spazio-temporale della colonia, situato qui, funziona ancora e conduce alla linea di frontiera su Terra Nova.",
            b"El viejo portal de teletransporte de la colonia, situado aqu\xed, a\xfan funciona y lleva al frente en Terra Nova.",
            b"De oude warppoort van de kolonie, die zich hier bevindt, werkt nog en leidt naar de frontlinie op Terra Nova.",
        ],
    ),
    (
        881,
        [
            b"The colony's factories still work, and crystals lie across its islands. Resupply here before you leave for the front.",
            b"Les usines de la colonie fonctionnent encore, et des cristaux sont dispers\xe9s sur ses \xeeles. Ravitaillez-vous ici avant de partir pour le front.",
            b"Die Fabriken der Kolonie arbeiten noch, und auf ihren Inseln liegen Kristalle. Versorgen Sie sich hier, bevor Sie an die Front aufbrechen.",
            b"Gli stabilimenti della colonia funzionano ancora e sulle sue isole si trovano dei cristalli. Rifornisciti qui prima di partire per il fronte.",
            b"Las f\xe1bricas de la colonia a\xfan funcionan y hay cristales repartidos por sus islas. Reabast\xe9cete aqu\xed antes de partir hacia el frente.",
            b"De fabrieken van de kolonie werken nog en op de eilanden liggen kristallen. Vul hier je voorraad aan voordat je naar het front vertrekt.",
        ],
    ),
    (
        882,
        [
            b"Night falls quickly on Pelagia, so make the most of the daylight.",
            b"La nuit tombe vite sur Pelagia : profitez de la lumi\xe8re du jour.",
            b"Die Nacht bricht schnell \xfcber Pelagia herein: Nutzen Sie das Tageslicht.",
            b"Su Pelagia la notte cala in fretta: approfitta della luce del giorno.",
            b"En Pelagia la noche cae deprisa: aprovecha la luz del d\xeda.",
            b"Op Pelagia valt de nacht snel: maak gebruik van het daglicht.",
        ],
    ),
    (
        896,
        [
            b"1 Resupply with crystals, ammunition and any upgrades you find",
            b"1 Faire le plein de cristaux, de munitions et d'\xe9quipements",
            b"1 Versorgen Sie sich mit Kristallen, Munition und Upgrades",
            b"1 Rifornirsi di cristalli, munizioni e potenziamenti",
            b"1 Reabastecerte de cristales, munici\xf3n y mejoras",
            b"1 Vul je kristallen, munitie en opwaarderingen aan",
        ],
    ),
    (
        897,
        [
            b"2 Warp to the front line on Terra Nova",
            b"2 Regagner le front sur Terra Nova par le portail intersid\xe9ral",
            b"2 Warpen Sie nach Terra Nova an die Front",
            b"2 Raggiungere la linea di frontiera su Terra Nova",
            b"2 Teletransportarte al frente en Terra Nova",
            b"2 Warp naar de frontlinie op Terra Nova",
        ],
    ),
    (
        898,
        [
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
        ],
    ),
    (
        899,
        [
            b"",
            b"",
            b"",
            b"",
            b"",
            b"",
        ],
    ),
];

/// The Command Centre's overlay, whose model bank world 1 borrows for its
/// ships (their model 665, which only levels 25 and 28 register).
pub const MODEL_BANK_LEVEL: u8 = 28;

/// Whether the loading world borrows the Command Centre's model bank:
/// only world 1 with the egg on. Anything else drops the loan, so only
/// world 1's draws can see it.
pub(crate) fn model_loan_wanted(on: bool, world: u32) -> bool {
    on && world == WORLD
}

/// A second residency activated at the Command Centre's overlay, so the
/// world draw borrows its models as the database's model fix borrows a
/// second residency's. Loading happens with the world, so the draw only
/// reads it.
pub(crate) fn borrowed_model_residency(
    installation: &Installation,
) -> Result<crate::session_platform::DatabaseModelResidency, crate::Error> {
    let mut runtime = ResidentRuntime::new(installation.clone())?;
    runtime.apply(ResidencyRequest::Boot)?;
    runtime.apply(ResidencyRequest::Activate {
        level: MODEL_BANK_LEVEL,
    })?;
    Ok(crate::session_platform::DatabaseModelResidency {
        level: MODEL_BANK_LEVEL,
        runtime,
    })
}

/// World 1's start with the rework on: out of the moved return gate. None
/// keeps the world's own (H1 start, or a gate's override).
pub(crate) fn spawn(
    world: u32,
    on: bool,
    rework: bool,
) -> Option<crate::harness_start::HarnessSpawn> {
    (on && rework && world == WORLD).then_some(crate::harness_start::HarnessSpawn {
        position: Some(SPAWN),
        heading: Some(SPAWN_HEADING),
    })
}

/// The return gate onto the dead gate's site, its ground's height, and the
/// dead gate's record where the return gate stood. The gate keeps its
/// authored facing, whose back looks over the island toward the colony.
fn trade_gate_places(arena: &mut ObjectArena, gate: ObjectId, terrain: &mut TerrainRuntime) {
    let height = terrain.height_at(GATE_SITE) as u16;
    if let Err(error) = arena.copy_actor_position(gate, [GATE_SITE[0], height, GATE_SITE[1]]) {
        eprintln!("Pelagia: the return gate {gate:?} stays where it stood: {error}");
        return;
    }
    match terrain.remove_furniture(GATE_SITE) {
        Some(record) if record.resource_lookup.raw() == DEAD_GATE_LOOKUP => {}
        Some(record) => eprintln!("Pelagia: the gate's site held {record:?}, not the dead gate"),
        None => eprintln!("Pelagia: no dead gate at {GATE_SITE:?}"),
    }
    if !terrain.add_furniture(
        DEAD_GATE_SITE,
        DEAD_GATE_STATE,
        inf_formats::TerrainResourceLookup::from_raw(DEAD_GATE_LOOKUP),
    ) {
        eprintln!("Pelagia: the dead gate finds no cell at {DEAD_GATE_SITE:?}");
    }
}

/// The world's name where a gate, the free look or the Teleport list names
/// world `world`: Pelagia for world 1 with the egg on; None leaves retail's
/// H0 `0xE0 + world` (world 1's reads "Completed", the campaign end's).
pub fn world_name(world: u32, on: bool) -> Option<&'static [u8]> {
    (on && world == WORLD).then_some(NAME)
}

/// The briefing program for a world coming up: the port's own for Pelagia
/// with the egg on (the rework's or retail's world 1's, by `rework`), else
/// the world's authored one. World 1 ships none, so without this its
/// briefing would exit at once.
pub(crate) fn briefing_program_for(
    selection: u32,
    on: bool,
    rework: bool,
    retail: &BriefingProgram,
) -> BriefingProgram {
    if on && selection == WORLD {
        briefing_program(rework)
    } else {
        retail.clone()
    }
}

/// When Pelagia's objectives list hands over to Start: as long after its
/// last objective as retail's list waits after its fourth (rows show a
/// second apart from its opening, the list closes at 5 s), so its two
/// objectives leave no empty rows on screen (switching from objectives to start was too slow). None for every other
/// world, which keeps retail's 5 s.
pub(crate) fn list_close_millis(selection: u32, on: bool, rework: bool) -> Option<u32> {
    if !on || selection != WORLD {
        return None;
    }
    let objectives = (OBJECTIVE..OBJECTIVE + 4)
        .filter(|slot| text(WORLD, 0, *slot, rework).is_some_and(|line| !line.is_empty()))
        .count() as u32;
    Some((objectives + 1) * 1000)
}

/// World 1's briefing, in the Command Centre's style: the camera opens over
/// the colony at the start, flies to the return gate, then (with the
/// rework) the start factory and the gate factory, and back to the colony;
/// the model pane shows the gate (H8 62) and the hovercraft factory (H8
/// 157, both authored initial models); the map pane opens at the Command
/// Centre's scale. The texts are the port's ([`text`]).
///
/// Each line stays up until the next, for about 5.8 s per 100 characters
/// of its longest language, 4 s at least: retail's lines take a median of
/// 5.9 s per 100 English characters and never under 3.6 s, where this
/// program's first cut flashed four of them past in 2 to 2.5 s, far too quick.
/// The briefing's views of the gate and the factories: each eye stands
/// this far back from its subject against the record's heading, so it
/// frames the gate or the factory instead of sitting inside it. Retail's cameras stand on their actor; its briefings aim
/// them at markers beside what they show.
const BRIEFING_STAND_OFF: u32 = 1600;
/// The factories are smaller than the gate: their eyes stand nearer, so
/// each sits near the pane's middle.
const FACTORY_STAND_OFF: u32 = 1200;
/// How fast the briefing's camera crosses to each view: about 6,000 world
/// units a second, so even the long hop from the gate to the colony lands
/// early in its line (retail's 300 took six seconds of it).
const VIEW_SPEED: u16 = 800;
const GATE_VIEW: u32 =
    crate::briefing_camera::stand_off_reference(RETURN_GATE as u16 + 1, BRIEFING_STAND_OFF);
const START_FACTORY_VIEW: u32 =
    crate::briefing_camera::stand_off_reference(START_FACTORY as u16 + 1, FACTORY_STAND_OFF);
const GATE_FACTORY_VIEW: u32 =
    crate::briefing_camera::stand_off_reference(GATE_FACTORY as u16 + 1, FACTORY_STAND_OFF);

pub(crate) fn briefing_program(rework: bool) -> BriefingProgram {
    let mut records = vec![
        record(0, 0, 8, 3, camera(3, 180, 360, 50_000, 0, 0)),
        record(0, 10, 5, 0, BriefingPayload::Visibility { flags: 8 }),
        record(0, 10, 3, 0, BriefingPayload::Visibility { flags: 4 }),
        record(0, 120, 0, 1, menu_text(875)),
        record(0, 105, 0, 1, menu_text(876)),
        record(0, 10, 0, 1, menu_text(877)),
        record(0, 10, 4, 0, BriefingPayload::Visibility { flags: 4 }),
        record(0, 75, 10, 4, BriefingPayload::MapScale { word: 64 }),
        record(0, 10, 0, 1, menu_text(878)),
        record(0, 30, 1, 0, BriefingPayload::Visibility { flags: 4 }),
        record(0, 75, 8, 3, camera(GATE_VIEW, 130, 200, VIEW_SPEED, 1, 1)),
    ];
    if rework {
        records.extend([
            record(0, 10, 0, 1, menu_text(879)),
            record(0, 50, 2, 2, model(62, [0, 0], 1000, [0, -50])),
            // The gate stays in view while its line is up.
            record(0, 65, 0, 1, menu_text(880)),
            record(
                0,
                15,
                8,
                3,
                camera(START_FACTORY_VIEW, 90, 200, VIEW_SPEED, 1, 1),
            ),
            record(0, 10, 2, 2, model(157, [0, 0], 1500, [0, -50])),
            record(0, 85, 0, 1, menu_text(881)),
            record(
                0,
                15,
                8,
                3,
                camera(GATE_FACTORY_VIEW, 270, 200, VIEW_SPEED, 1, 1),
            ),
            record(0, 75, 0, 1, menu_text(882)),
            record(
                0,
                15,
                8,
                3,
                camera(START_FACTORY_VIEW, 90, 200, VIEW_SPEED, 1, 1),
            ),
        ]);
    } else {
        records.extend([
            record(0, 10, 0, 1, menu_text(879)),
            record(0, 70, 2, 2, model(62, [0, 0], 1000, [0, -50])),
            record(0, 75, 0, 1, menu_text(880)),
            record(
                0,
                15,
                8,
                3,
                camera(START_FACTORY_VIEW, 90, 200, VIEW_SPEED, 1, 1),
            ),
            record(0, 85, 0, 1, menu_text(881)),
            record(0, 45, 0, 1, menu_text(882)),
        ]);
    }
    records.extend([
        record(0, 40, 0, 1, menu_text(883)),
        record(0, 10, 1, 0, BriefingPayload::Visibility { flags: 1 }),
        record(0, 10, 4, 0, BriefingPayload::Visibility { flags: 1 }),
        record(0, 10, 10, 4, BriefingPayload::MapScale { word: 0 }),
    ]);
    for (index, record) in records.iter_mut().enumerate() {
        record.index = index as u32;
    }
    BriefingProgram {
        record_span: ByteSpan::new(0, 0),
        records,
    }
}

fn record(
    index: u32,
    duration_100ms: u32,
    raw_kind: u32,
    raw_word_08: u32,
    payload: BriefingPayload,
) -> BriefingRecord {
    BriefingRecord {
        index,
        span: ByteSpan::new(0, 0),
        duration_100ms,
        raw_kind,
        raw_word_08,
        payload_offset: 0,
        payload_span: ByteSpan::new(0, 0),
        payload,
    }
}

fn menu_text(string_id: u16) -> BriefingPayload {
    BriefingPayload::MenuText {
        string_id,
        increment_bytes: [0, 0],
    }
}

fn camera(
    actor_id: u32,
    heading_degrees: i16,
    rotation_word: i16,
    speed_word: u16,
    track_byte: u8,
    terrain_byte: u8,
) -> BriefingPayload {
    BriefingPayload::Camera(BriefingCamera {
        actor_id,
        heading_degrees,
        rotation_word,
        speed_word,
        track_byte,
        terrain_byte,
    })
}

fn model(
    model_slot: i16,
    position_xy: [i16; 2],
    depth_bias: i16,
    screen_shift_per_thousand: [i16; 2],
) -> BriefingPayload {
    BriefingPayload::Model(BriefingModel {
        model_slot,
        position_xy,
        depth_bias,
        screen_shift_per_thousand,
    })
}

/// How many times faster the rework's day falls to night than retail's
/// schedule (day, day, sunset, night in 400-second steps, night by 20
/// minutes): on 2026-10-26 made faster, to 10 minutes of gameplay instead
/// of 20, then on 2026-10-27 to half that, which was still slow. Four times: the day holds to 1:40, the
/// sunset is in by 3:20, the dark comes from about 3:40 and night is whole
/// by about 4:45.
const DAY_PACE: i32 = 4;

/// World 1's tint schedule period, seconds, under the rework: retail's
/// `period` compressed by [`DAY_PACE`], so night falls by 5 minutes.
pub(crate) fn rework_tint_period(period: i32) -> i32 {
    (period / DAY_PACE).max(1)
}

/// After a world loads with the egg on: the Command Centre's gate 81 is
/// marked so a scientist answers it while its lock holds (positions and
/// facings stay retail's, nothing moves); with the rework on, world 1's
/// gate leads back to the Command Centre with no scientist asked for,
/// its start factory stocks the hovercraft and its gate factory the
/// headlights, and its egg crystals are born across the islands.
/// Anything missing stays retail's. Returns the born egg crystals, if any.
pub(crate) fn prepare_world(
    arena: &mut ObjectArena,
    world: u32,
    placements: &[Option<ObjectId>],
    residency: &ResidentRuntime,
    terrain: &mut TerrainRuntime,
    rework: bool,
) -> Vec<ObjectId> {
    if world == COMMAND_CENTRE {
        // The Command Centre keeps its retail positions and facings: no
        // gate is moved here. Gate 81 is still marked so a scientist
        // answers it while its lock holds and wakes it; world 1's single
        // gate stays its only gate, with nothing to trade places with.
        if let Some(dead) = placements.get(DEAD_GATE).and_then(|slot| *slot) {
            arena.mark_pelagia_gate(dead);
        }
        return Vec::new();
    }
    if world != WORLD {
        return Vec::new();
    }
    if !rework {
        // Briefing and name only: the map stays retail's.
        return Vec::new();
    }
    eprintln!(
        "Pelagia: {} cells of the crater isle and its sandbar sculpted",
        ground::sculpt(terrain)
    );
    carve_pelagia_lagoon(arena, placements, terrain);
    eprintln!(
        "Pelagia: {} ruins sunk in the lagoon",
        place(terrain, RUINS)
    );
    eprintln!(
        "Pelagia: {} pieces in the crater",
        place(terrain, CRATER_PIECES)
    );
    if let Some(Some(guard)) = placements.get(GUARD) {
        let height = terrain.height_at(GUARD_DEN) as u16;
        if let Err(error) = arena.copy_actor_position(*guard, [GUARD_DEN[0], height, GUARD_DEN[1]])
        {
            eprintln!("Pelagia: the quixle {guard:?} keeps the gate's island: {error}");
        }
    }
    if let Some(Some(gate)) = placements.get(RETURN_GATE) {
        if let Err(error) = arena.prepare_pelagia_return_gate(*gate) {
            eprintln!("Pelagia: the return gate {gate:?} stays retail's: {error}");
        } else {
            trade_gate_places(arena, *gate, terrain);
        }
    }
    if let Some(Some(trigger)) = placements.get(RETURN_HINT) {
        if let Err(error) = arena.dispose(*trigger) {
            eprintln!("Pelagia: the return gate's hint {trigger:?} stays: {error}");
        }
    }
    if let Some(Some(factory)) = placements.get(START_FACTORY) {
        if let Err(error) = arena.prepare_pelagia_factory(*factory, HOVERCRAFT, true) {
            eprintln!("Pelagia: the start factory {factory:?} holds nothing new: {error}");
        }
    }
    if let Some(Some(factory)) = placements.get(GATE_FACTORY) {
        if let Err(error) = arena.prepare_pelagia_factory(*factory, HEADLIGHTS, false) {
            eprintln!("Pelagia: the gate factory {factory:?} holds nothing new: {error}");
        }
    }
    // The colony's scientists each tell a line of its story as they board
    // (the last colonist, the plan's ideas), in placement order.
    let mut told = 0u8;
    for scientist in placements.iter().flatten() {
        if arena.mark_pelagia_colonist(*scientist, told).is_ok() {
            told += 1;
        }
    }
    let mut born = birth_pelagia_pickups(
        arena,
        terrain,
        residency,
        CRYSTAL_SPOTS,
        FIRST_SYNTHETIC_PLACEMENT,
    );
    eprintln!("Pelagia: {} egg crystals born", born.len());
    let trails = trail_spots();
    born.extend(birth_pelagia_pickups(
        arena,
        terrain,
        residency,
        &trails,
        FIRST_SYNTHETIC_PLACEMENT + CRYSTAL_SPOTS.len() as u32,
    ));
    born.extend(birth_sunk_tokens(
        arena,
        terrain,
        residency,
        FIRST_SYNTHETIC_PLACEMENT + (CRYSTAL_SPOTS.len() + trails.len()) as u32,
    ));
    born
}

/// One piece of furniture the rework adds: map X/Z, the record's state
/// byte (its turn in the high nibble, as the map's own records hold it)
/// and its furniture lookup in world 1's own placement table.
struct Piece {
    x: u16,
    z: u16,
    state: u8,
    lookup: u8,
}

/// The colony's sunken corner on the lagoon bed, beside the token trail's
/// cache (the plan's ideas: "ruins on the lagoon bed"): a store hut, the
/// colony's statue, an old pier's posts walking out from the shore, and a
/// stone, all world 1's own furniture (stores, statue02, post01, post02,
/// post04, post05, stone05), placed as records of the port's own. Each
/// takes an even cell, as every authored record does: the coarse ground's
/// furniture walk visits even rows and keeps even columns.
const RUINS: &[Piece] = &[
    // The old pier, from the shore (8 under) out to 76 under.
    Piece {
        x: 35328,
        z: 32512,
        state: 0x00,
        lookup: 64,
    },
    Piece {
        x: 35072,
        z: 32512,
        state: 0x00,
        lookup: 65,
    },
    Piece {
        x: 34816,
        z: 32512,
        state: 0x00,
        lookup: 67,
    },
    Piece {
        x: 34560,
        z: 32512,
        state: 0x00,
        lookup: 68,
    },
    // The store hut, 100 under.
    Piece {
        x: 33536,
        z: 32000,
        state: 0x00,
        lookup: 3,
    },
    // The colony's statue by the cache, 104 under.
    Piece {
        x: 34048,
        z: 32000,
        state: 0x00,
        lookup: 35,
    },
    // A stone, 84 under.
    Piece {
        x: 34304,
        z: 32256,
        state: 0x00,
        lookup: 73,
    },
];

/// The crater's shrine: a statue of the colony's at the floor's heart,
/// palms round the floor (world 1's palm01 to palm04) and stones (stone01,
/// stone03, stone04) on the rim either side of the notch the road comes
/// through.
const CRATER_PIECES: &[Piece] = &[
    // The statue (statue03).
    Piece {
        x: 51456,
        z: 40192,
        state: 0x40,
        lookup: 36,
    },
    // The palms.
    Piece {
        x: 52224,
        z: 40448,
        state: 0x00,
        lookup: 14,
    },
    Piece {
        x: 51712,
        z: 41216,
        state: 0x30,
        lookup: 16,
    },
    Piece {
        x: 50944,
        z: 41216,
        state: 0x60,
        lookup: 15,
    },
    Piece {
        x: 50688,
        z: 39424,
        state: 0x90,
        lookup: 17,
    },
    Piece {
        x: 51712,
        z: 39168,
        state: 0xc0,
        lookup: 14,
    },
    Piece {
        x: 52224,
        z: 39680,
        state: 0xf0,
        lookup: 16,
    },
    // The stones on the rim either side of the notch, clear of the road.
    Piece {
        x: 49152,
        z: 39936,
        state: 0x00,
        lookup: 69,
    },
    Piece {
        x: 49664,
        z: 39680,
        state: 0x50,
        lookup: 71,
    },
    Piece {
        x: 50176,
        z: 41728,
        state: 0xa0,
        lookup: 72,
    },
];

/// The dormant quixle the map sets by the gate's outpost (placement 19),
/// and where the rework moves it: the crater's floor, by the shrine it
/// guards, so the player comes out of the gate in peace and wakes it at
/// the cache.
const GUARD: usize = 19;
const GUARD_DEN: [u16; 2] = [51900, 40900];

/// Place `pieces` on the ground. Returns how many took their cell.
fn place(terrain: &mut TerrainRuntime, pieces: &[Piece]) -> usize {
    pieces
        .iter()
        .filter(|piece| {
            terrain.add_furniture(
                [piece.x, piece.z],
                piece.state,
                inf_formats::TerrainResourceLookup::from_raw(piece.lookup),
            )
        })
        .count()
}

/// One of the rework's night lights: map X/Z, its height over the ground,
/// colour, strength and reach, and whether it pulses.
struct NightLight {
    x: u16,
    z: u16,
    height: i32,
    color: [u8; 3],
    strength: f32,
    reach: f32,
    pulse: bool,
}

/// The colony's windows and its factories' pads in warm light, the return
/// gate in a cool one so the way home shows after dark, and a beacon over
/// the outer islets whose crystals only the hovercraft reaches, pulsing
/// across the water (the plan's ideas: "the colony lit at night" and "a
/// beacon on the outer islets"), and a lamp at the crater's shrine.
/// Lights of the tracer's own: retail's
/// terrain lights are gated off, and the raster draws none of these.
const NIGHT_LIGHTS: &[NightLight] = &[
    // The colony huts by the start, where three of its scientists stand.
    NightLight {
        x: 38900,
        z: 28150,
        height: 120,
        color: [255, 190, 120],
        strength: 0.8,
        reach: 550.0,
        pulse: false,
    },
    NightLight {
        x: 39450,
        z: 28450,
        height: 120,
        color: [255, 190, 120],
        strength: 0.8,
        reach: 550.0,
        pulse: false,
    },
    // Factory 2's pad, and factory 17's by the gate.
    NightLight {
        x: 36900,
        z: 28250,
        height: 150,
        color: [255, 205, 150],
        strength: 0.8,
        reach: 600.0,
        pulse: false,
    },
    NightLight {
        x: 27050,
        z: 25700,
        height: 150,
        color: [255, 205, 150],
        strength: 0.8,
        reach: 600.0,
        pulse: false,
    },
    // The return gate, on the east island.
    NightLight {
        x: GATE_SITE[0] + 250,
        z: GATE_SITE[1] + 250,
        height: 160,
        color: [150, 210, 255],
        strength: 0.9,
        reach: 650.0,
        pulse: false,
    },
    // The crater's shrine, a lamp the colonists left burning.
    NightLight {
        x: 51456,
        z: 40192,
        height: 260,
        color: [255, 200, 140],
        strength: 1.1,
        reach: 900.0,
        pulse: false,
    },
    // The beacon over the north-east islets.
    NightLight {
        x: 49950,
        z: 14400,
        height: 220,
        color: [255, 240, 200],
        strength: 1.5,
        reach: 900.0,
        pulse: true,
    },
];

/// The night lights' glow id: no model's, and not the shots' (which warm
/// what they light under thermal vision).
const NIGHT_LIGHT_GLOWS: u16 = u16::MAX - 1;

/// The rework's night lights for the traced world: eye-relative centres
/// and their lights, none by day. The beacon pulses once every two
/// seconds of `clock_micros`.
pub(crate) fn night_lights(
    tint: [u8; 3],
    clock_micros: u64,
    eye: [i32; 3],
    terrain: &TerrainRuntime,
) -> Vec<([f32; 3], crate::scene_feed::SceneGlow)> {
    let night = crate::world_scene::nightfall(tint);
    if night <= 0.0 {
        return Vec::new();
    }
    let phase = (clock_micros % 2_000_000) as f32 / 2_000_000.0 * std::f32::consts::TAU;
    NIGHT_LIGHTS
        .iter()
        .map(|light| {
            let ground = i32::from(terrain.height_at([light.x, light.z]));
            let world = [
                i32::from(light.x),
                ground + light.height,
                i32::from(light.z),
            ];
            let centre =
                std::array::from_fn(|axis| world[axis].wrapping_sub(eye[axis]) as i16 as f32);
            let pulse = if light.pulse {
                0.55 + 0.45 * phase.sin()
            } else {
                1.0
            };
            let [r, g, b] = light.color;
            (
                centre,
                crate::scene_feed::SceneGlow {
                    texture: None,
                    color: [r, g, b, 255],
                    strength: light.strength * night * pulse,
                    reach: light.reach,
                    model: NIGHT_LIGHT_GLOWS,
                    ..Default::default()
                },
            )
        })
        .collect()
}

/// H13's shared pickup constructor: retail's green crystals use it, with a
/// kind command and the default value.
const PICKUP_RESOURCE: u32 = 0xffff_0009;
/// First synthetic placement index: past the 74 authored ones (and the
/// briefing cameras' actor IDs), so nothing references one.
const FIRST_SYNTHETIC_PLACEMENT: u32 = 1000;
/// Pickup kinds: blue and red crystals (green is 15).
const BLUE_CRYSTAL: u16 = 14;
const RED_CRYSTAL: u16 = 16;
/// Born pickups stand on the ground, as retail's placed pickups do (their
/// y equals the sampled ground exactly).
/// Ground must clear the water plane by this to hold a crystal.
const CRYSTAL_SHORE_MARGIN: i32 = 16;

/// One egg crystal: map coordinates and red or blue kind. A few caches,
/// each where the way leads: by each factory, at the gate the player comes
/// out of, a pair on each outer islet (the hovercraft's), and a ring round
/// the shrine in the crater isle's floor, the egg's biggest cache.
#[derive(Clone, Copy)]
struct CrystalSpot {
    x: u16,
    z: u16,
    kind: u16,
}

const CRYSTAL_SPOTS: &[CrystalSpot] = &[
    // Two red by factory 2, with the green ones already on the map.
    CrystalSpot {
        x: 36100,
        z: 28900,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 37750,
        z: 27700,
        kind: RED_CRYSTAL,
    },
    // Two blue by factory 17.
    CrystalSpot {
        x: 26200,
        z: 25200,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 27500,
        z: 26400,
        kind: BLUE_CRYSTAL,
    },
    // A red and a blue at the head of the gate's trail.
    CrystalSpot {
        x: 50200,
        z: 27500,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 50750,
        z: 27800,
        kind: BLUE_CRYSTAL,
    },
    // A pair on each outer islet, which only the hovercraft reaches.
    CrystalSpot {
        x: 50300,
        z: 14650,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 50050,
        z: 14480,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 39900,
        z: 11700,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 39400,
        z: 11200,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 41000,
        z: 17600,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 41100,
        z: 18100,
        kind: BLUE_CRYSTAL,
    },
    // The ring round the crater's shrine, red and blue in turn.
    CrystalSpot {
        x: 51880,
        z: 40399,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 51599,
        z: 40680,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 51201,
        z: 40680,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 50920,
        z: 40399,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 50920,
        z: 40001,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 51201,
        z: 39720,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 51599,
        z: 39720,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 51880,
        z: 40001,
        kind: BLUE_CRYSTAL,
    },
];

/// Token kind: the yellow orbs, as retail's token trails place them.
const TOKEN: u16 = 7;

/// The gate's trail, as retail's trails lead between its sights: from the
/// gate west between the outpost's barracks, round the south end of its
/// trench and over the neck to the colony's east gate.
const TOKEN_SPOTS: &[CrystalSpot] = &[
    CrystalSpot {
        x: 50300,
        z: 27900,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 49532,
        z: 28459,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 49045,
        z: 29217,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 48490,
        z: 29876,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 47646,
        z: 29692,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 46824,
        z: 29215,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 45982,
        z: 28784,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 45084,
        z: 28479,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 44149,
        z: 28309,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 43204,
        z: 28381,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 42258,
        z: 28467,
        kind: TOKEN,
    },
];

/// Every trail token: the gate's ([`TOKEN_SPOTS`]), then the crater
/// road's ([`ground::road_tokens`]).
fn trail_spots() -> Vec<CrystalSpot> {
    TOKEN_SPOTS
        .iter()
        .copied()
        .chain(
            ground::road_tokens()
                .into_iter()
                .map(|[x, z]| CrystalSpot { x, z, kind: TOKEN }),
        )
        .collect()
}

/// The tidal-flats lagoon, in world units: two rectangles ringing the
/// colony's west and south. Cells in them that stand below the high-water
/// mark drop by one step; cells holding anything placed or born keep
/// their ground, so no authored or egg item floats or sinks.
/// A token trail from the lagoon's south shore down onto its bed, glowing
/// up through the water as it deepens (the traced water shows sunk pickups), to a sunken cache at its end: two red
/// crystals and a blue, the scarce ones. Retail sinks tokens only where the
/// buggy and the truck still reach them (world 18's
/// lie 72 and 83 deep), so these keep to the shallows, 16 to 90 under the
/// plane once the lagoon is carved, and the carving does not keep clear of
/// them.
const SUNK_TOKEN_SPOTS: &[CrystalSpot] = &[
    // On the shelf off the beach, 20 under.
    CrystalSpot {
        x: 35900,
        z: 32300,
        kind: TOKEN,
    },
    // Then west along the bed, 64, 48 and 77 under.
    CrystalSpot {
        x: 35500,
        z: 32156,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 35000,
        z: 32156,
        kind: TOKEN,
    },
    CrystalSpot {
        x: 34500,
        z: 32156,
        kind: TOKEN,
    },
    // The cache, on the shelf past the trail's end, 44 to 70 under.
    CrystalSpot {
        x: 34000,
        z: 32250,
        kind: RED_CRYSTAL,
    },
    CrystalSpot {
        x: 33800,
        z: 32300,
        kind: BLUE_CRYSTAL,
    },
    CrystalSpot {
        x: 33650,
        z: 32200,
        kind: RED_CRYSTAL,
    },
];
/// How deep a sunk token may lie: under the shore's wash, and no deeper
/// than the ground forms reach.
const SUNK_DEPTHS: std::ops::RangeInclusive<i32> = 16..=90;

const LAGOON_WEST: [u32; 4] = [33000, 26000, 35000, 30500];
const LAGOON_SOUTH: [u32; 4] = [33000, 30500, 36500, 32200];
/// Lower bound (deep channels stay deep) and high-water mark, relative to
/// the plane, and the drop applied.
const LAGOON_FLOOR: i32 = -208;
const LAGOON_SHORE: i32 = 48;
const LAGOON_DROP: i32 = 112;
/// Nothing placed or born moves: keep this clearance around every item.
const LAGOON_CLEARANCE: i32 = 256;

/// Egg crystals for world 1: red and blue pickups the map never places.
/// Each is constructed through the shared pickup constructor and given its
/// kind, like the game's own drops, but with a placement index past the
/// authored ones, so it takes a pickup serial and collects persistently
/// like a placed pickup instead of returning every load. Water spots are
/// skipped. Returns what was born. Only the tests birth them now.
#[cfg(test)]
pub(crate) fn birth_pelagia_crystals(
    arena: &mut ObjectArena,
    terrain: &TerrainRuntime,
    residency: &ResidentRuntime,
) -> Vec<ObjectId> {
    birth_pelagia_pickups(
        arena,
        terrain,
        residency,
        CRYSTAL_SPOTS,
        FIRST_SYNTHETIC_PLACEMENT,
    )
}

/// The lagoon: deepen the colony's tidal flats into water. Cells in the
/// lagoon rectangles standing below the high-water mark drop one step,
/// except those holding anything placed or born, which keep their ground
/// so nothing floats or sinks. Runs before any birth, so the egg finds
/// the carved ground.
pub(crate) fn carve_pelagia_lagoon(
    arena: &mut ObjectArena,
    placements: &[Option<ObjectId>],
    terrain: &mut TerrainRuntime,
) {
    let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
    let mut keep: Vec<[i32; 2]> = Vec::new();
    for slot in placements.iter().flatten() {
        if let Ok(object) = arena.object(*slot) {
            if let Some([x, _, z]) = object.base.position_words {
                keep.push([i32::from(x), i32::from(z)]);
            }
        }
    }
    for spot in CRYSTAL_SPOTS.iter().chain(TOKEN_SPOTS.iter()) {
        keep.push([i32::from(spot.x), i32::from(spot.z)]);
    }
    let mut carved = 0u32;
    for rect in &[LAGOON_WEST, LAGOON_SOUTH] {
        let [x0, z0, x1, z1] = *rect;
        let (mut cx, cz1) = (x0 / 128, z0 / 128);
        while cx <= x1 / 128 {
            let mut cz = cz1;
            while cz <= z1 / 128 {
                let x = cx * 128 + 64;
                let z = cz * 128 + 64;
                if x >= x0
                    && x <= x1
                    && z >= z0
                    && z <= z1
                    && !keep.iter().any(|[kx, kz]| {
                        (x as i32 - kx).abs() <= LAGOON_CLEARANCE
                            && (z as i32 - kz).abs() <= LAGOON_CLEARANCE
                    })
                {
                    let ground = i32::from(terrain.raw_height_at([x as u16, z as u16]));
                    if ground >= plane + LAGOON_FLOOR && ground < plane + LAGOON_SHORE {
                        terrain.write_raw_height_128([x as u16, z as u16], ground - LAGOON_DROP);
                        carved += 1;
                    }
                }
                cz += 1;
            }
            cx += 1;
        }
    }
    eprintln!("Pelagia: lagoon carved out of {carved} cells");
}

/// Pickups born after the load: each is constructed through the shared
/// pickup constructor and given its kind, like the game's own drops, but
/// with a placement index past the authored ones, so it takes a pickup
/// serial and collects persistently like a placed pickup instead of
/// returning every load. Water spots are skipped. Returns what was born.
fn birth_pelagia_pickups(
    arena: &mut ObjectArena,
    terrain: &TerrainRuntime,
    residency: &ResidentRuntime,
    spots: &[CrystalSpot],
    first_index: u32,
) -> Vec<ObjectId> {
    birth_pickups(arena, terrain, residency, spots, first_index, false)
}

/// The lagoon's sunk token trail and its cache ([`SUNK_TOKEN_SPOTS`]), each
/// born on the carved bed where the water over it is [`SUNK_DEPTHS`] deep.
fn birth_sunk_tokens(
    arena: &mut ObjectArena,
    terrain: &TerrainRuntime,
    residency: &ResidentRuntime,
    first_index: u32,
) -> Vec<ObjectId> {
    birth_pickups(
        arena,
        terrain,
        residency,
        SUNK_TOKEN_SPOTS,
        first_index,
        true,
    )
}

/// [`birth_pelagia_pickups`] on land, or (`under_water`) on the bed of
/// shallow water only.
fn birth_pickups(
    arena: &mut ObjectArena,
    terrain: &TerrainRuntime,
    residency: &ResidentRuntime,
    spots: &[CrystalSpot],
    first_index: u32,
    under_water: bool,
) -> Vec<ObjectId> {
    let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
    let mut born = Vec::new();
    for (index, spot) in spots.iter().enumerate() {
        let ground = i32::from(terrain.height_at([spot.x, spot.z]));
        if under_water && !SUNK_DEPTHS.contains(&(plane - ground)) {
            eprintln!(
                "Pelagia: sunk token spot ({}, {}) lies {} under the water, skipping",
                spot.x,
                spot.z,
                plane - ground
            );
            continue;
        }
        if !under_water && ground < plane + CRYSTAL_SHORE_MARGIN {
            eprintln!(
                "Pelagia: crystal spot ({}, {}) is water, skipping",
                spot.x, spot.z
            );
            continue;
        }
        let height = match u16::try_from(ground) {
            Ok(height) => height,
            Err(_) => {
                eprintln!(
                    "Pelagia: crystal spot ({}, {}) is out of reach, skipping",
                    spot.x, spot.z
                );
                continue;
            }
        };
        let placement = first_index + index as u32;
        match construct_world_object(
            arena,
            residency,
            ObjectConstructionRequest {
                resource_argument: PICKUP_RESOURCE,
                context: BaseContext {
                    position_words: Some([spot.x, height, spot.z]),
                    placement_index: placement,
                },
                suffix: None,
                command_allocation: None,
                network_active: false,
                actor: None,
            },
        ) {
            Ok(ObjectConstructionOutcome::Constructed(object)) => {
                match arena.select_pickup_kind(object, spot.kind, false, residency) {
                    Ok(true) => born.push(object),
                    Ok(false) => eprintln!(
                        "Pelagia: crystal at ({}, {}) takes no known kind",
                        spot.x, spot.z
                    ),
                    Err(error) => eprintln!(
                        "Pelagia: crystal at ({}, {}) takes no kind: {error:?}",
                        spot.x, spot.z
                    ),
                }
            }
            outcome => eprintln!(
                "Pelagia: crystal at ({}, {}) not born: {outcome:?}",
                spot.x, spot.z
            ),
        }
    }
    born
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_report_has_six_languages_in_latin_1() {
        for language in 0..6 {
            let text = report(language);
            assert!(text.len() > 40 && !text.contains(&0));
        }
        assert_eq!(report(9), report(0));
        assert!(report(0).starts_with(b"Commander,"));
        assert!(report(1).starts_with(b"Commandant,"));
        assert!(report(2).starts_with(b"Commander,"));
        assert!(report(3).starts_with(b"Comandante,"));
        assert!(report(4).starts_with(b"Comandante,"));
        assert!(report(5).starts_with(b"Commandant,"));
    }

    #[test]
    fn only_world_1_with_the_egg_on_is_named_pelagia() {
        assert_eq!(world_name(1, true), Some(NAME));
        assert_eq!(world_name(1, false), None);
        assert_eq!(world_name(18, true), None);
    }

    /// Every slot the briefing reads, rework on and off, but for the
    /// objectives past the first two.
    const SLOTS: [i32; 13] = [
        TITLE,
        875,
        876,
        877,
        878,
        879,
        880,
        881,
        882,
        883,
        OBJECTIVE,
        897,
        HEADER_NAME,
    ];

    #[test]
    fn pelagia_texts_cover_every_slot_in_every_language() {
        for rework in [true, false] {
            for language in 0..6 {
                for slot in SLOTS {
                    let text = text(WORLD, language, slot, rework).unwrap_or_else(|| {
                        panic!("world 1 language {language} slot {slot} rework {rework}")
                    });
                    assert!(!text.is_empty(), "language {language} slot {slot}");
                    assert!(!text.contains(&0), "language {language} slot {slot}");
                }
            }
        }
        // Two objectives either way, the other rows empty as retail's
        // shorter lists leave theirs.
        for language in 0..6 {
            for rework in [true, false] {
                assert!(!text(WORLD, language, 897, rework).unwrap().is_empty());
                assert_eq!(text(WORLD, language, 898, rework), Some(b"".as_slice()));
                assert_eq!(text(WORLD, language, 899, rework), Some(b"".as_slice()));
            }
        }
        assert_eq!(text(COMMAND_CENTRE, 0, TITLE, true), None);
        // Retail's convention past the six: English.
        assert_eq!(
            text(WORLD, 9, FIRST_LINE, true),
            text(WORLD, 0, FIRST_LINE, true)
        );
    }

    #[test]
    fn pelagia_texts_name_the_world_and_follow_the_plan() {
        for language in 0..6 {
            assert_eq!(text(WORLD, language, TITLE, true), Some(NAME));
            assert_eq!(text(WORLD, language, HEADER_NAME, false), Some(NAME));
        }
        assert!(text(WORLD, 0, 875, true)
            .unwrap()
            .starts_with(b"This is Pelagia"));
        assert!(contains(text(WORLD, 1, 882, true).unwrap(), b"feux avant"));
        assert!(text(WORLD, 2, 883, false)
            .unwrap()
            .starts_with(b"Genie\xdfen"));
        assert!(contains(text(WORLD, 3, 881, true).unwrap(), b"hovercraft"));
        assert!(contains(
            text(WORLD, 4, 878, true).unwrap(),
            b"Centro de Mando"
        ));
        assert!(text(WORLD, 5, 875, false)
            .unwrap()
            .starts_with(b"Dit is Pelagia"));
        // The hovercraft is a form the factory gives, as the buggy is: no
        // objective builds one.
        assert!(text(WORLD, 0, OBJECTIVE, true)
            .unwrap()
            .starts_with(b"1 Resupply"));
        assert!(text(WORLD, 0, 897, true)
            .unwrap()
            .starts_with(b"2 Warp back to the Command Centre"));
        assert!(contains(
            text(WORLD, 0, 881, true).unwrap(),
            b"hovercraft mode"
        ));
        // How the player came: the scientist's repair at gate 81, and the
        // alien creatures the map places (a Nemian, a quixle).
        assert!(contains(text(WORLD, 0, 878, true).unwrap(), b"scientist"));
        assert!(contains(text(WORLD, 0, 877, false).unwrap(), b"creatures"));
    }

    /// With the rework off, world 1's gate is retail's: it needs no
    /// scientist and leads to Terra Nova, and its factories hold nothing
    /// new, so no line asks for a scientist or promises the way home, the
    /// hovercraft or headlights.
    #[test]
    fn retail_world_1_lines_tell_its_own_gate() {
        for language in 0..6 {
            for slot in 879..=899 {
                let Some(line) = text(WORLD, language, slot, false) else {
                    continue;
                };
                assert!(
                    !contains(line, b"hovercraft"),
                    "language {language} slot {slot}"
                );
                if language == 0 {
                    assert!(!contains(line, b"scientist"), "slot {slot}");
                }
            }
            assert!(contains(
                text(WORLD, language, 880, false).unwrap(),
                b"Terra Nova"
            ));
            assert!(contains(
                text(WORLD, language, 897, false).unwrap(),
                b"Terra Nova"
            ));
        }
        assert!(contains(text(WORLD, 0, 881, false).unwrap(), b"crystals"));
        assert!(!contains(
            text(WORLD, 0, 882, false).unwrap(),
            b"headlights"
        ));
    }

    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle)
    }

    /// The program's lines in order, each with how long it stays up: its
    /// own record and every one after it up to the next line.
    fn line_times(program: &BriefingProgram) -> Vec<(u16, u32)> {
        let mut lines = Vec::new();
        for record in &program.records {
            match record.payload {
                BriefingPayload::MenuText { string_id, .. } => {
                    lines.push((string_id, record.duration_100ms))
                }
                _ => {
                    if let Some((_, time)) = lines.last_mut() {
                        *time += record.duration_100ms;
                    }
                }
            }
        }
        lines
    }

    /// Each line stays up for at least 5.5 s per 100 characters of its
    /// longest language, and 4 s however short (retail's lines: a median
    /// of 5.9 s per 100 English characters, never under 3.6 s).
    #[test]
    fn every_line_stays_up_long_enough_to_read() {
        for rework in [true, false] {
            let program = briefing_program(rework);
            for (slot, time) in line_times(&program) {
                let longest = (0..6)
                    .map(|language| {
                        text(WORLD, language, i32::from(slot), rework)
                            .unwrap()
                            .len()
                    })
                    .max()
                    .unwrap();
                assert!(time >= 40, "slot {slot} up {time} (rework {rework})");
                assert!(
                    time as usize * 100 >= longest * 55,
                    "slot {slot}: {longest} characters up {time} (rework {rework})"
                );
            }
        }
    }

    #[test]
    fn the_briefing_mirrors_the_command_centre_shape() {
        for rework in [true, false] {
            let program = briefing_program(rework);
            for (index, record) in program.records.iter().enumerate() {
                assert_eq!(record.index as usize, index);
            }
            let lines: Vec<u16> = line_times(&program)
                .into_iter()
                .map(|(slot, _)| slot)
                .collect();
            assert_eq!(lines, (875..=883).collect::<Vec<_>>());
            let cameras: Vec<u32> = program
                .records
                .iter()
                .filter_map(|record| match record.payload {
                    BriefingPayload::Camera(camera) => {
                        Some(camera.actor_id & !crate::briefing_camera::STAND_OFF_FLAG & 0xffff)
                    }
                    _ => None,
                })
                .collect();
            let models: Vec<i16> = program
                .records
                .iter()
                .filter_map(|record| match record.payload {
                    BriefingPayload::Model(model) => Some(model.model_slot),
                    _ => None,
                })
                .collect();
            // The colony at the start (factory 2, placement index plus
            // one), gate 73, and with the rework factory 17 too, each
            // after the first seen from a stand-off.
            if rework {
                assert_eq!(cameras, [3, 74, 3, 18, 3]);
                assert_eq!(models, [62, 157]);
            } else {
                assert_eq!(cameras, [3, 74, 3]);
                assert_eq!(models, [62]);
            }
            let scales: Vec<u16> = program
                .records
                .iter()
                .filter_map(|record| match record.payload {
                    BriefingPayload::MapScale { word } => Some(word),
                    _ => None,
                })
                .collect();
            assert_eq!(scales, [64, 0]);
            // It closes as the Command Centre's does: the terrain and model
            // panes, then the map's scale.
            let tail: Vec<u32> = program.records[program.records.len() - 3..]
                .iter()
                .map(|record| record.raw_kind)
                .collect();
            assert_eq!(tail, [1, 4, 10]);
        }
    }

    /// The list hands over to Start two seconds after its last objective,
    /// as retail's does after its fourth: 3 s for either briefing's two;
    /// retail's worlds keep their 5 s.
    #[test]
    fn the_objectives_hand_over_as_soon_as_retails_do() {
        assert_eq!(list_close_millis(WORLD, true, true), Some(3000));
        assert_eq!(list_close_millis(WORLD, true, false), Some(3000));
        assert_eq!(list_close_millis(WORLD, false, true), None);
        assert_eq!(list_close_millis(COMMAND_CENTRE, true, true), None);
        assert_eq!(
            crate::briefing_runtime::LIST_CLOSE_MILLIS,
            (4 + 1) * 1000,
            "retail's four rows and its wait"
        );
    }

    #[test]
    fn briefing_program_for_swaps_only_pelagia_with_the_egg_on() {
        let retail = BriefingProgram {
            record_span: ByteSpan::new(0, 0),
            records: Vec::new(),
        };
        assert_eq!(
            briefing_program_for(WORLD, true, true, &retail),
            briefing_program(true)
        );
        assert_eq!(
            briefing_program_for(WORLD, true, false, &retail),
            briefing_program(false)
        );
        assert!(briefing_program_for(WORLD, false, true, &retail)
            .records
            .is_empty());
        assert!(briefing_program_for(COMMAND_CENTRE, true, true, &retail)
            .records
            .is_empty());
    }

    #[test]
    fn prepare_world_retargets_the_return_gate_and_stocks_the_factories() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        let gate = placements[RETURN_GATE].expect("world 1 places gate 73");
        let state = session.arena.selector30_actor(gate).unwrap();
        assert_eq!((state.word_15c, state.word_160), (COMMAND_CENTRE, 1));
        assert_eq!(session.arena.actor(gate).unwrap().word_b4, 2);
        let trigger = placements[RETURN_HINT].expect("world 1 places hint 44");
        // 00479FA0 queues the disposal; the world's first tick drains the
        // queue before any actor callback, so the hint can never fire.
        assert!(session
            .arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .any(|node| node.object == trigger));
        let start = placements[START_FACTORY].expect("world 1 places factory 2");
        assert_eq!(session.arena.actor(start).unwrap().word_7a, HOVERCRAFT);
        assert!(inf_formats::grant_is_owned(
            &session.arena.selector15(start).unwrap().grants_198.mask_04,
            u32::from(HOVERCRAFT)
        )
        .unwrap());
        let gate_factory = placements[GATE_FACTORY].expect("world 1 places factory 17");
        assert_eq!(
            session.arena.actor(gate_factory).unwrap().word_7a,
            STANDARD_GRENADES
        );
        assert!(inf_formats::grant_is_owned(
            &session
                .arena
                .selector15(gate_factory)
                .unwrap()
                .grants_198
                .mask_04,
            u32::from(HEADLIGHTS)
        )
        .unwrap());
    }

    #[test]
    fn egg_crystals_spawn_on_land_with_kinds_models_values_and_serials() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        assert_eq!(
            CRYSTAL_SPOTS
                .iter()
                .filter(|spot| spot.kind == RED_CRYSTAL)
                .count(),
            10
        );
        assert_eq!(
            CRYSTAL_SPOTS
                .iter()
                .filter(|spot| spot.kind == BLUE_CRYSTAL)
                .count(),
            10
        );
        let born = birth_pelagia_crystals(&mut session.arena, &session.terrain, &runtime);
        assert_eq!(born.len(), CRYSTAL_SPOTS.len(), "every spot is land");
        let plane =
            i32::from((session.terrain.plane_base() >> session.terrain.plane_shift()) as i16);
        let mut serials = std::collections::BTreeSet::new();
        let (mut red, mut blue) = (0, 0);
        for (index, (object, spot)) in born.iter().zip(CRYSTAL_SPOTS.iter()).enumerate() {
            let pickup = session.arena.timed_pickup(*object).unwrap();
            assert_eq!(pickup.kind_44, Some(spot.kind));
            assert_eq!(
                pickup.visual.model_word,
                if spot.kind == RED_CRYSTAL { 0xf9 } else { 0xfb }
            );
            assert_eq!(pickup.value_46, 1);
            assert_ne!(pickup.save_index_48, u32::MAX);
            assert!(
                serials.insert(pickup.save_index_48),
                "pickup serials stay unique"
            );
            match spot.kind {
                RED_CRYSTAL => red += 1,
                BLUE_CRYSTAL => blue += 1,
                kind => panic!("spot kind {kind}"),
            }
            let base = &session.arena.object(*object).unwrap().base;
            assert_eq!(
                u32::from(base.placement_word),
                FIRST_SYNTHETIC_PLACEMENT + index as u32
            );
            let [x, y, z] = base.position_words.expect("crystals stand placed");
            assert_eq!((x, z), (spot.x, spot.z));
            let ground = i32::from(session.terrain.height_at([x, z]));
            assert!(
                ground >= plane + CRYSTAL_SHORE_MARGIN,
                "spot ({x}, {z}) is land"
            );
            assert_eq!(i32::from(y), ground);
        }
        assert_eq!((red, blue), (10, 10));
    }

    #[test]
    fn token_trails_follow_the_gate_trail_and_the_crater_road() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        assert!(ground::sculpt(&mut session.terrain) > 0);
        let trails = trail_spots();
        assert_eq!(TOKEN_SPOTS.len(), 11);
        assert_eq!(
            trails.len(),
            TOKEN_SPOTS.len() + ground::road_tokens().len()
        );
        let born = birth_pelagia_pickups(
            &mut session.arena,
            &session.terrain,
            &runtime,
            &trails,
            FIRST_SYNTHETIC_PLACEMENT,
        );
        assert_eq!(born.len(), trails.len(), "every trail spot is land");
        let plane =
            i32::from((session.terrain.plane_base() >> session.terrain.plane_shift()) as i16);
        let mut serials = std::collections::BTreeSet::new();
        for (index, (object, spot)) in born.iter().zip(trails.iter()).enumerate() {
            let pickup = session.arena.timed_pickup(*object).unwrap();
            assert_eq!(pickup.kind_44, Some(TOKEN));
            assert_eq!(pickup.visual.model_word, 0x3b);
            assert_eq!(pickup.value_46, 1);
            assert_ne!(pickup.save_index_48, u32::MAX);
            assert!(serials.insert(pickup.save_index_48));
            let base = &session.arena.object(*object).unwrap().base;
            assert_eq!(
                u32::from(base.placement_word),
                FIRST_SYNTHETIC_PLACEMENT + index as u32
            );
            let [x, y, z] = base.position_words.expect("tokens stand placed");
            assert_eq!((x, z), (spot.x, spot.z));
            let ground = i32::from(session.terrain.height_at([x, z]));
            assert!(ground >= plane + CRYSTAL_SHORE_MARGIN);
            assert_eq!(i32::from(y), ground);
        }
    }

    /// The lagoon floods the colony's tidal flats but touches nothing
    /// placed or born: every authored position keeps its exact ground, the
    /// egg and trail spots stay land, and carved cells hold water. Land
    /// routes reachable before stay reachable after.
    #[test]
    fn pelagia_lagoon_floods_flats_and_spares_placements() {
        use std::collections::{HashSet, VecDeque};
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        let plane =
            i32::from((session.terrain.plane_base() >> session.terrain.plane_shift()) as i16);
        let placed_grounds: Vec<(u16, u16, i32)> = placements
            .iter()
            .flatten()
            .filter_map(|object| {
                session
                    .arena
                    .object(*object)
                    .ok()
                    .and_then(|state| state.base.position_words)
                    .map(|[x, _, z]| (x, z, i32::from(session.terrain.height_at([x, z]))))
            })
            .collect();
        fn reachable(
            terrain: &crate::terrain_runtime::TerrainRuntime,
            plane: i32,
        ) -> HashSet<[u16; 2]> {
            let mut seen = HashSet::new();
            let mut open = VecDeque::new();
            open.push_back([37536u16, 28000u16]);
            seen.insert([37536u16, 28000u16]);
            while let Some([x, z]) = open.pop_front() {
                for (dx, dz) in [(256i32, 0), (-256, 0), (0, 256), (0, -256)] {
                    let (nx, nz) = (x as i32 + dx, z as i32 + dz);
                    if nx < 8000 || nx > 60000 || nz < 8000 || nz > 50000 {
                        continue;
                    }
                    let next = [nx as u16, nz as u16];
                    if seen.contains(&next) {
                        continue;
                    }
                    if i32::from(terrain.height_at(next)) >= plane + CRYSTAL_SHORE_MARGIN {
                        seen.insert(next);
                        open.push_back(next);
                    }
                }
            }
            seen
        }
        // The flood walks a coarse 256-grid from the start, so a point of
        // interest rarely sits exactly on it: probe its nearest grid cell.
        // The exact ground is checked cell by cell above and below.
        fn snap(point: [u16; 2]) -> [u16; 2] {
            let snap_axis = |value: u16, origin: i32| {
                let step = ((i32::from(value) - origin) as f32 / 256.0).round() as i32;
                (origin + step * 256) as u16
            };
            [snap_axis(point[0], 37536), snap_axis(point[1], 28000)]
        }
        let before = reachable(&session.terrain, plane);
        // Factory 2 holds the hovercraft and stands on the colony island:
        // the start walks there. Factory 17 and gate 73 stand off it, on
        // hovercraft country across open water even in retail, so no
        // overland flood reaches them; their exact ground is checked cell
        // by cell above and below instead.
        for poi in [[36736, 28400]] {
            let cell = snap(poi);
            assert!(
                (i32::from(cell[0]) - i32::from(poi[0])).abs() <= 128
                    && (i32::from(cell[1]) - i32::from(poi[1])).abs() <= 128,
                "probe cell {cell:?} stays beside point {poi:?}"
            );
            assert!(
                before.contains(&cell),
                "start reaches {poi:?} (via {cell:?})"
            );
        }
        let born = prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        assert_eq!(
            born.len(),
            CRYSTAL_SPOTS.len() + trail_spots().len() + SUNK_TOKEN_SPOTS.len()
        );
        for (x, z, ground) in &placed_grounds {
            assert_eq!(
                i32::from(session.terrain.height_at([*x, *z])),
                *ground,
                "placed ground at ({x}, {z}) never moves"
            );
        }
        let after = reachable(&session.terrain, plane);
        for poi in [[36736, 28400]] {
            assert!(after.contains(&snap(poi)), "start still reaches {poi:?}");
        }
        assert_eq!(
            after.contains(&snap([28000, 19000])),
            before.contains(&snap([28000, 19000])),
            "the lagoon neither strands nor ferries gate 73"
        );
        // The lagoon holds water where the flats stood.
        let mut wet = 0u32;
        for x0 in (33000..=41000).step_by(512) {
            for z0 in (26000..=32000).step_by(512) {
                if i32::from(session.terrain.height_at([x0 as u16, z0 as u16])) < plane {
                    wet += 1;
                }
            }
        }
        assert!(wet > 20, "the lagoon holds water, {wet} soundings wet");
    }

    /// The lagoon's sunk trail: every token is born on the carved bed,
    /// under water but within the ground forms' reach, deepening from the
    /// shore, each a token with its own pickup serial.
    #[test]
    fn the_sunk_trail_runs_from_the_shore_onto_the_lagoon_bed() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        let born = prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        let terrain = &session.terrain;
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        let sunk = &born[born.len() - SUNK_TOKEN_SPOTS.len()..];
        let mut depths = Vec::new();
        for (object, spot) in sunk.iter().zip(SUNK_TOKEN_SPOTS) {
            let pickup = session.arena.timed_pickup(*object).unwrap();
            assert_eq!(pickup.kind_44, Some(spot.kind));
            let base = &session.arena.object(*object).unwrap().base;
            let [x, y, z] = base.position_words.expect("born placed");
            assert_eq!([x, z], [spot.x, spot.z]);
            let depth = plane - i32::from(y as i16);
            assert!(SUNK_DEPTHS.contains(&depth), "({x}, {z}) lies {depth} deep");
            depths.push(depth);
        }
        let tokens = SUNK_TOKEN_SPOTS
            .iter()
            .filter(|spot| spot.kind == TOKEN)
            .count();
        assert!(
            depths[0] < depths[tokens - 1],
            "the trail deepens from the shore: {depths:?}"
        );
        // Its end holds the scarce crystals: two red and a blue.
        let cache: Vec<u16> = SUNK_TOKEN_SPOTS[tokens..]
            .iter()
            .map(|spot| spot.kind)
            .collect();
        assert_eq!(cache, [RED_CRYSTAL, BLUE_CRYSTAL, RED_CRYSTAL]);
    }

    /// The rework marks each of world 1's scientists with its own line of
    /// the colony's story, told once as it boards; nothing is marked with
    /// the rework off.
    #[test]
    fn the_colonists_each_tell_a_line_of_the_story_once() {
        for rework in [true, false] {
            let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
                WORLD,
                crate::terrain_runtime::TerrainSpacing::Units256,
            );
            let placements = session.report.placements.clone();
            prepare_world(
                &mut session.arena,
                WORLD,
                &placements,
                &runtime,
                &mut session.terrain,
                rework,
            );
            let mut lines = Vec::new();
            for slot in placements.iter().flatten() {
                if session.arena.selector12_actor(*slot).is_ok() {
                    if let Some(line) = session.arena.take_pelagia_story(*slot) {
                        lines.push(line);
                        assert_eq!(session.arena.take_pelagia_story(*slot), None, "told once");
                    }
                }
            }
            if rework {
                assert_eq!(
                    lines,
                    (0..6).collect::<Vec<u8>>(),
                    "six colonists, six lines"
                );
            } else {
                assert!(lines.is_empty());
            }
        }
        for language in 0..6 {
            for line in 0..6 {
                let text = colonist_line(line, language);
                assert!(!text.is_empty() && !text.contains(&0));
                assert!(text.len() <= 116, "line {line} language {language}");
            }
        }
        assert_eq!(colonist_line(6, 0), colonist_line(0, 0));
        assert_eq!(colonist_line(0, 9), colonist_line(0, 0));
        assert!(colonist_line(4, 0).starts_with(b"Something sank in the lagoon"));
    }

    /// The rework's night falls by 5 minutes: retail's 400-second steps a
    /// quarter as long, so its four styles (day, day, sunset, night) are
    /// spent by 300 seconds.
    #[test]
    fn the_rework_night_falls_in_a_quarter_of_the_time() {
        assert_eq!(rework_tint_period(400), 100);
        assert_eq!(3 * rework_tint_period(400), 300);
        assert_eq!(rework_tint_period(1), 1);
    }

    /// The rework's night lights stay off by day and through the sunset,
    /// come on as the sunset fades to night, and the islets' beacon pulses.
    #[test]
    fn the_colony_lights_come_on_as_night_falls() {
        // World 1's schedule: day, sunset, night.
        use crate::world_scene::nightfall;
        assert_eq!(nightfall([186, 133, 105]), 0.0);
        assert_eq!(nightfall([220, 117, 89]), 0.0);
        assert_eq!(nightfall([32, 28, 57]), 1.0);
        let dusk = nightfall([110, 75, 72]);
        assert!(dusk > 0.0 && dusk < 1.0, "half-lit at dusk: {dusk}");
        let (_runtime, _assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let eye = [37536, 600, 28000];
        assert!(night_lights([186, 133, 105], 0, eye, &session.terrain).is_empty());
        let lights = night_lights([32, 28, 57], 0, eye, &session.terrain);
        assert_eq!(lights.len(), NIGHT_LIGHTS.len());
        for ((centre, glow), light) in lights.iter().zip(NIGHT_LIGHTS) {
            let ground = i32::from(session.terrain.height_at([light.x, light.z]));
            assert_eq!(centre[1] as i32, ground + light.height - eye[1]);
            assert_eq!(glow.texture, None);
            assert_ne!(glow.model, crate::world_scene::SHOT_GLOWS, "no shot's heat");
        }
        // The beacon swells and fades over two seconds; the rest hold.
        let beacon = |micros| {
            night_lights([32, 28, 57], micros, eye, &session.terrain)
                .last()
                .unwrap()
                .1
                .strength
        };
        assert!(beacon(500_000) > beacon(1_500_000));
        let steady = |micros| {
            night_lights([32, 28, 57], micros, eye, &session.terrain)[0]
                .1
                .strength
        };
        assert_eq!(steady(500_000), steady(1_500_000));
    }

    /// The rework sinks the colony's ruins as furniture records of its
    /// own: each in its row's bucket on a marked even cell, where the
    /// coarse ground's walk finds it, under the water; none without it.
    #[test]
    fn the_ruins_lie_on_the_lagoon_bed_as_furniture() {
        for rework in [true, false] {
            let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
                WORLD,
                crate::terrain_runtime::TerrainSpacing::Units256,
            );
            let before = session
                .terrain
                .buckets()
                .iter()
                .map(|bucket| bucket.records.len())
                .sum::<usize>();
            let placements = session.report.placements.clone();
            prepare_world(
                &mut session.arena,
                WORLD,
                &placements,
                &runtime,
                &mut session.terrain,
                rework,
            );
            let terrain = &session.terrain;
            let after = terrain
                .buckets()
                .iter()
                .map(|bucket| bucket.records.len())
                .sum::<usize>();
            if !rework {
                assert_eq!(after, before);
                continue;
            }
            assert_eq!(after, before + RUINS.len() + CRATER_PIECES.len());
            let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
            for ruin in RUINS {
                let [x, z] = [ruin.x >> 7, ruin.z >> 7];
                assert_eq!(
                    [x & 1, z & 1],
                    [0, 0],
                    "even cell for ({}, {})",
                    ruin.x,
                    ruin.z
                );
                assert!(terrain.cell_at([ruin.x, ruin.z]).has_sparse_record());
                let bucket = terrain
                    .buckets()
                    .iter()
                    .find(|bucket| bucket.y == z)
                    .unwrap();
                let record = bucket.records.iter().find(|record| record.x == x).unwrap();
                assert_eq!(record.resource_lookup.raw(), ruin.lookup);
                assert!(
                    bucket.records.windows(2).all(|pair| pair[0].x < pair[1].x),
                    "row {z} stays sorted"
                );
                let ground = i32::from(terrain.height_at([ruin.x + 64, ruin.z + 64]));
                assert!(
                    ground < plane,
                    "({}, {}) lies under the water",
                    ruin.x,
                    ruin.z
                );
            }
            // A cell that already holds furniture takes none.
            let mut terrain = session.terrain.clone();
            let ruin = &RUINS[0];
            assert!(!terrain.add_furniture(
                [ruin.x, ruin.z],
                0,
                inf_formats::TerrainResourceLookup::from_raw(3)
            ));
        }
    }

    #[test]
    fn prepare_world_marks_but_never_moves_the_command_centre_gate() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            COMMAND_CENTRE,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        let dead = placements[DEAD_GATE].expect("the Command Centre places gate 81");
        let live = placements[2].expect("the Command Centre places gate 2");
        let dead_position = session.arena.object(dead).unwrap().base.position_words;
        let live_position = session.arena.object(live).unwrap().base.position_words;
        assert_ne!(dead_position, live_position);
        let dead_basis = session.arena.actor(dead).unwrap().rotating.basis;
        let live_basis = session.arena.actor(live).unwrap().rotating.basis;
        let serials_before = session.arena.world_load_state().pickup_serial_54ed48;
        prepare_world(
            &mut session.arena,
            COMMAND_CENTRE,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        // Positions and facings stay retail's: no swap on world 18.
        assert_eq!(
            session.arena.object(dead).unwrap().base.position_words,
            dead_position
        );
        assert_eq!(
            session.arena.object(live).unwrap().base.position_words,
            live_position
        );
        assert_eq!(
            session.arena.actor(dead).unwrap().rotating.basis,
            dead_basis
        );
        assert_eq!(
            session.arena.actor(live).unwrap().rotating.basis,
            live_basis
        );
        // Destinations unchanged, and the dead gate is marked so it wakes.
        assert_eq!(session.arena.selector30_actor(dead).unwrap().word_15c, 0);
        let live_state = session.arena.selector30_actor(live).unwrap();
        assert_eq!((live_state.word_15c, live_state.word_160), (2, 1));
        assert!(session.arena.pelagia_gate_locked(dead));
        assert!(!session.arena.pelagia_gate_locked(live));
        // No crystals born outside world 1.
        assert_eq!(
            session.arena.world_load_state().pickup_serial_54ed48,
            serials_before
        );
    }

    #[test]
    fn prepare_world_without_rework_keeps_world_1_retail() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        // Sample the ground across the lagoon rectangles before the load.
        let mut grounds = Vec::new();
        for x in (33000..=36500).step_by(512) {
            for z in (26000..=32200).step_by(512) {
                grounds.push((
                    [x as u16, z as u16],
                    session.terrain.height_at([x as u16, z as u16]),
                ));
            }
        }
        let serials_before = session.arena.world_load_state().pickup_serial_54ed48;
        let born = prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            false,
        );
        // Nothing born, no ground moved: the briefing plays over the retail map.
        assert!(born.is_empty());
        assert_eq!(
            session.arena.world_load_state().pickup_serial_54ed48,
            serials_before
        );
        for ([x, z], ground) in &grounds {
            assert_eq!(&session.terrain.height_at([*x, *z]), ground);
        }
        // Gate 73 still leads to Terra Nova, and the factories hold retail's.
        let gate = placements[RETURN_GATE].expect("world 1 places gate 73");
        let state = session.arena.selector30_actor(gate).unwrap();
        assert_eq!((state.word_15c, state.word_160), (2, 1));
        let start = placements[START_FACTORY].expect("world 1 places factory 2");
        assert_ne!(session.arena.actor(start).unwrap().word_7a, HOVERCRAFT);
    }

    #[test]
    fn the_world_draw_borrows_model_665_from_the_command_centre_bank() {
        use crate::render_resources::{
            RenderResourceData, RenderResourceKind, RenderResourceRequest,
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../retail");
        let (runtime, _assets, _session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        // World 1 never registers the perships' model...
        assert!(runtime.resource(8, 665).is_none());
        let bank = borrowed_model_residency(&crate::Installation::open(&root).unwrap()).unwrap();
        assert_eq!(bank.level, MODEL_BANK_LEVEL);
        // ...while the Command Centre's bank does.
        let registration = bank
            .runtime
            .resource(8, 665)
            .expect("level 28 registers 665");
        assert_eq!(registration.registration.global_index, 665);
        // The draw resolves it through the fallback...
        let request = RenderResourceRequest {
            kind: RenderResourceKind::Model,
            argument: 665,
        };
        assert!(matches!(
            crate::render_resources::RenderResources::new(&runtime).resolve(request),
            Err(crate::render_resources::RenderResourceError::NotResident { .. })
        ));
        let resolved = crate::render_resources::RenderResources::new(&runtime)
            .with_model_fallback(&bank.runtime)
            .resolve(request)
            .unwrap();
        assert!(matches!(resolved.data, RenderResourceData::Model(_)));
        assert_eq!(resolved.source.registration.level, MODEL_BANK_LEVEL);
        // ...but models the world loads itself still come from its own bank.
        let own = crate::render_resources::RenderResources::new(&runtime)
            .with_model_fallback(&bank.runtime)
            .resolve(RenderResourceRequest {
                kind: RenderResourceKind::Model,
                argument: 62,
            })
            .unwrap();
        assert_ne!(own.source.registration.level, MODEL_BANK_LEVEL);
        // A slot neither bank registers still fails as not resident.
        let capacity = runtime.resource_capacity(8).unwrap() as i32;
        assert!(matches!(
            crate::render_resources::RenderResources::new(&runtime)
                .with_model_fallback(&bank.runtime)
                .resolve(RenderResourceRequest {
                    kind: RenderResourceKind::Model,
                    argument: capacity,
                }),
            Err(crate::render_resources::RenderResourceError::NotResident { .. })
        ));
        // The contact pass finds it the same way, so the ships stand solid
        // instead of stopping every pair contact with them.
        use crate::session_platform::contact_model;
        assert!(contact_model(&runtime, None, 665).unwrap().is_none());
        let ship = contact_model(&runtime, Some(&bank.runtime), 665)
            .unwrap()
            .expect("the borrowed bank answers the contact");
        assert!(
            ship.model.header.raw_0e > 0,
            "the ship has a contact radius"
        );
        // Only world 1 with the egg on borrows the bank.
        assert!(model_loan_wanted(true, WORLD));
        assert!(!model_loan_wanted(false, WORLD));
        assert!(!model_loan_wanted(true, COMMAND_CENTRE));
    }

    #[test]
    fn pelagia_landscape_recon_probe() {
        let (_runtime, _assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let terrain = &session.terrain;
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        let mut row = String::from("    ");
        for x in (44000..=57000).step_by(1500) {
            row.push_str(&format!("{:>5}", x / 1000));
        }
        eprintln!("{row}");
        let mut z = 34000;
        while z <= 47000 {
            let mut line = format!("{z:5}");
            let mut x = 44000;
            while x <= 57000 {
                let h = i32::from(terrain.height_at([x as u16, z as u16])) - plane;
                line.push_str(if h >= 16 { "    #" } else { "    ." });
                x += 1500;
            }
            eprintln!("{line}");
            z += 1500;
        }
        for (name, x0, z0, x1, z1) in [
            ("NW", 48800, 13300, 51600, 16200),
            ("N", 38600, 10400, 41000, 12800),
            ("NE", 39800, 16800, 42400, 19200),
        ] {
            eprintln!("{name}");
            let mut z = z0;
            while z <= z1 {
                let mut line = format!("{z:5}");
                let mut x = x0;
                while x <= x1 {
                    let h = i32::from(terrain.height_at([x as u16, z as u16])) - plane;
                    line.push_str(&format!("{:5}", h));
                    x += 400;
                }
                eprintln!("{line}");
                z += 400;
            }
        }
    }

    #[test]
    fn pelagia_landscape_recon_probe2() {
        let (_runtime, _assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let terrain = &session.terrain;
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        for i in 0..=20 {
            let x = (37536i32 - i * 800 / 20) as u16;
            let z = (28000i32 + i * 400 / 20) as u16;
            let h = i32::from(terrain.height_at([x, z])) - plane;
            eprintln!("along {x},{z}: {h}");
        }
    }

    #[test]
    fn prepare_world_leaves_other_worlds_alone() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            COMMAND_CENTRE,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        prepare_world(
            &mut session.arena,
            COMMAND_CENTRE,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        // Gate 2 still leads to Terra Nova, not the prototype world.
        let gate = placements[2].expect("the Command Centre places gate 2");
        let state = session.arena.selector30_actor(gate).unwrap();
        assert_eq!((state.word_15c, state.word_160), (2, 1));
    }

    #[test]
    fn world_strings_name_pelagia_over_retail() {
        use crate::menu_text::MenuTextResources;
        use crate::render_resources::{RenderResources, WorldStrings};
        let (runtime, _assets, _session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let plain = RenderResources::new(&runtime);
        let title = plain.string(TITLE).unwrap();
        assert!(title.starts_with(b"Command Centre"), "{title:?}");
        let pelagia = RenderResources::new(&runtime).with_world_strings(WorldStrings {
            world: WORLD,
            language: 0,
            rework: true,
        });
        assert_eq!(pelagia.string(TITLE).unwrap(), NAME);
        assert_eq!(pelagia.string(HEADER_NAME).unwrap(), NAME);
        assert!(pelagia.string(875).unwrap().starts_with(b"This is Pelagia"));
        assert!(pelagia
            .string(OBJECTIVE)
            .unwrap()
            .starts_with(b"1 Resupply"));
        let french = RenderResources::new(&runtime).with_world_strings(WorldStrings {
            world: WORLD,
            language: 1,
            rework: false,
        });
        assert!(french
            .string(OBJECTIVE)
            .unwrap()
            .starts_with(b"1 Faire le plein"));
        assert!(french.string(875).unwrap().starts_with(b"Voici Pelagia"));
        // A slot the port does not own still reads retail's bytes.
        assert_eq!(pelagia.string(873), plain.string(873));
    }

    #[test]
    fn pelagia_se_island_recon_probe() {
        let (_runtime, _assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let terrain = &session.terrain;
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        let mut row = String::from("     ");
        for x in (48000..=54000).step_by(1000) {
            row.push_str(&format!("{:>6}", x));
        }
        eprintln!("{row}");
        let mut z = 37000;
        while z <= 43000 {
            let mut line = format!("{z:5}");
            let mut x = 48000;
            while x <= 54000 {
                let h = i32::from(terrain.height_at([x as u16, z as u16])) - plane;
                line.push_str(&format!("{:6}", h));
                x += 1000;
            }
            eprintln!("{line}");
            z += 1000;
        }
    }

    #[test]
    fn the_briefing_camera_can_find_its_world_1_targets() {
        let (_runtime, _assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let mut state = crate::briefing_camera::BriefingCameraState::default();
        state.activate(3, &session.arena).unwrap();
        assert_eq!(state.target_xz_words(), [36736, 28400]);
        state.activate(74, &session.arena).unwrap();
        assert_eq!(state.target_xz_words(), [28000, 19000]);
        state.activate(18, &session.arena).unwrap();
        assert_eq!(state.target_xz_words(), [26880, 25856]);
    }

    /// The briefing's views stand back from the gate and the factories,
    /// looking at them along each record's heading, instead of putting the
    /// eye inside them.
    #[test]
    fn the_briefing_frames_the_gate_and_factories_from_in_front() {
        let (_runtime, assets, session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let math = crate::menu_basis::MenuBasisMath::new(assets.sine_table().values);
        let heading = |degrees: i32| degrees * 0x1_0000 / 360;
        let mut state = crate::briefing_camera::BriefingCameraState::default();
        for (reference, degrees, subject, stand_off) in [
            (GATE_VIEW, 130, [28000.0, 19000.0], BRIEFING_STAND_OFF),
            (
                START_FACTORY_VIEW,
                90,
                [36736.0, 28400.0],
                FACTORY_STAND_OFF,
            ),
            (
                GATE_FACTORY_VIEW,
                270,
                [26880.0, 25856.0],
                FACTORY_STAND_OFF,
            ),
        ] {
            state
                .activate_standing_off(reference, heading(degrees), &math, &session.arena)
                .unwrap();
            let [x, z] = state.target_xz_words().map(f64::from);
            let angle = f64::from(degrees).to_radians();
            let eye = [
                subject[0] - angle.sin() * f64::from(stand_off),
                subject[1] - angle.cos() * f64::from(stand_off),
            ];
            assert!(
                (x - eye[0]).abs() < 3.0 && (z - eye[1]).abs() < 3.0,
                "{reference:#x}: eye at ({x}, {z}), wanted {eye:?}"
            );
        }
        // Retail's references keep their actor's own place.
        state
            .activate_standing_off(74, heading(130), &math, &session.arena)
            .unwrap();
        assert_eq!(state.target_xz_words(), [28000, 19000]);
    }

    /// With the rework, the return gate stands on the dead gate's site, the
    /// dead gate's record where the return gate stood, and the ACAM comes
    /// out 450 behind the gate on dry land, facing away from it.
    #[test]
    fn the_gates_trade_places_and_the_acam_comes_out_of_the_working_one() {
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        assert_eq!(
            session
                .terrain
                .sparse_record_at(GATE_SITE)
                .map(|(_, record)| record.resource_lookup.raw()),
            Some(DEAD_GATE_LOOKUP),
            "retail's dead gate"
        );
        prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        let terrain = &session.terrain;
        let gate = placements[RETURN_GATE].unwrap();
        let [x, y, z] = session
            .arena
            .object(gate)
            .unwrap()
            .base
            .position_words
            .unwrap();
        assert_eq!([x, z], GATE_SITE);
        assert_eq!(y as i16, terrain.height_at(GATE_SITE));
        assert!(terrain.sparse_record_at(GATE_SITE).is_none());
        let (_, dead) = terrain.sparse_record_at(DEAD_GATE_SITE).unwrap();
        assert_eq!(dead.resource_lookup.raw(), DEAD_GATE_LOOKUP);
        assert_eq!(dead.state, DEAD_GATE_STATE);
        // Retail's gate start: 450 along the basis' third row, backwards.
        let basis = session.arena.actor(gate).unwrap().rotating.basis;
        let row = [basis.words[7], basis.words[9]].map(f32::from_bits);
        for axis in 0..2 {
            let expected = f32::from(GATE_SITE[axis]) - 450.0 * row[axis];
            assert!(
                (f32::from(SPAWN[axis]) - expected).abs() < 2.0,
                "axis {axis}"
            );
        }
        // Facing away: the heading's forward is the gate's back.
        let radians = f32::from(SPAWN_HEADING).to_radians();
        let forward = [radians.sin(), radians.cos()];
        assert!(forward[0] * -row[0] + forward[1] * -row[1] > 0.99);
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);
        assert!(i32::from(terrain.height_at(SPAWN)) > plane + 40, "dry land");
    }

    #[test]
    fn only_the_reworked_world_1_starts_at_the_gate() {
        let start = spawn(WORLD, true, true).unwrap();
        assert_eq!(start.position, Some(SPAWN));
        assert_eq!(start.heading, Some(SPAWN_HEADING));
        assert!(spawn(WORLD, true, false).is_none());
        assert!(spawn(WORLD, false, true).is_none());
        assert!(spawn(COMMAND_CENTRE, true, true).is_none());
    }

    /// Map survey sheets for choosing the landscape: a shaded-relief
    /// top-down of world 1 with water and every placed item marked, a
    /// colony zoom, and a legend. Written to .tmp/pelagia/survey/.
    #[test]
    fn pelagia_map_survey_writes_sheets() {
        use std::collections::{BTreeMap, HashSet};
        let (runtime, _assets, mut session) = crate::test_worlds::offline_world(
            WORLD,
            crate::terrain_runtime::TerrainSpacing::Units256,
        );
        let placements = session.report.placements.clone();
        let born = prepare_world(
            &mut session.arena,
            WORLD,
            &placements,
            &runtime,
            &mut session.terrain,
            true,
        );
        assert_eq!(
            born.len(),
            CRYSTAL_SPOTS.len() + trail_spots().len() + SUNK_TOKEN_SPOTS.len()
        );
        let terrain = &session.terrain;
        let plane = i32::from((terrain.plane_base() >> terrain.plane_shift()) as i16);

        // Every live placement, classified for the markers.
        let disposed: HashSet<_> = session
            .arena
            .list(crate::object_runtime::ObjectList::Disposal)
            .iter()
            .map(|node| node.object)
            .collect();
        // Category, color and count.
        let mut markers: Vec<(u16, u16, [u8; 3], bool)> = Vec::new();
        let mut counts: BTreeMap<&'static str, usize> = BTreeMap::new();
        let mut note = |category: &'static str, x: u16, z: u16, color: [u8; 3], egg: bool| {
            *counts.entry(category).or_insert(0) += 1;
            markers.push((x, z, color, egg));
        };
        for slot in placements.iter().flatten() {
            if disposed.contains(slot) {
                continue;
            }
            let base = &session.arena.object(*slot).unwrap().base;
            let Some([x, _, z]) = base.position_words else {
                continue;
            };
            if session.arena.selector30_actor(*slot).is_ok() {
                note("gate", x, z, [255, 255, 0], false);
            } else if session.arena.selector15(*slot).is_ok() {
                note("factory", x, z, [255, 128, 0], false);
            } else if session.arena.selector12_actor(*slot).is_ok() {
                note("scientist", x, z, [0, 255, 255], false);
            } else if session.arena.selector34_actor(*slot).is_ok() {
                note("pership", x, z, [255, 0, 255], false);
            } else if let Ok(pickup) = session.arena.timed_pickup(*slot) {
                let color = match pickup.kind_44 {
                    Some(7) => [255, 255, 128],
                    Some(14) => [0, 128, 255],
                    Some(15) => [0, 255, 0],
                    Some(16) => [255, 0, 0],
                    _ => [128, 128, 128],
                };
                let name = match pickup.kind_44 {
                    Some(7) => "token",
                    Some(14) => "blue",
                    Some(15) => "green",
                    Some(16) => "red",
                    _ => "pickup",
                };
                note(name, x, z, color, false);
            } else if session.arena.selector50(*slot).is_ok() {
                note("hint", x, z, [128, 128, 128], false);
            } else {
                note("scenery", x, z, [64, 64, 64], false);
            }
        }
        for object in &born {
            let base = &session.arena.object(*object).unwrap().base;
            let Some([x, _, z]) = base.position_words else {
                continue;
            };
            let kind = session.arena.timed_pickup(*object).unwrap().kind_44;
            let (name, color) = match kind {
                Some(7) => ("token", [255, 255, 128]),
                Some(14) => ("egg-blue", [0, 128, 255]),
                Some(16) => ("egg-red", [255, 0, 0]),
                _ => ("egg", [255, 255, 255]),
            };
            note(name, x, z, color, true);
        }
        assert_eq!(counts.get("gate"), Some(&1));
        assert_eq!(counts.get("factory"), Some(&2));
        assert_eq!(counts.get("scientist"), Some(&6));
        assert_eq!(counts.get("pership"), Some(&5));
        assert_eq!(counts.get("hint"), Some(&16));
        assert_eq!(counts.get("token"), Some(&52));
        assert_eq!(counts.get("green"), Some(&15));
        assert_eq!(counts.get("egg-red"), Some(&12));
        assert_eq!(counts.get("egg-blue"), Some(&11));

        fn shade(height: i32, plane: i32, gx: i32, gz: i32) -> [u8; 3] {
            if height <= plane {
                let f = ((plane - height).min(700) as f32 / 700.0).min(1.0);
                let mix = |shallow: u8, deep: u8| {
                    (f32::from(shallow) + (f32::from(deep) - f32::from(shallow)) * f) as u8
                };
                return [mix(36, 6), mix(122, 28), mix(168, 78)];
            }
            let t = height - plane;
            let base = if t < 48 {
                [216, 200, 148]
            } else if t < 420 {
                let f = (t - 48) as f32 / 372.0;
                let mix = |low: u8, high: u8| {
                    (f32::from(low) + (f32::from(high) - f32::from(low)) * f) as u8
                };
                [mix(106, 58), mix(168, 128), mix(78, 58)]
            } else {
                [124, 118, 104]
            };
            let facing = -(gx + gz) as f32;
            let brightness = (0.78 + 0.30 * (facing / 512.0).clamp(-1.0, 1.0)).min(1.0);
            base.map(|channel| (f32::from(channel) * brightness) as u8)
        }

        fn render(
            terrain: &crate::terrain_runtime::TerrainRuntime,
            plane: i32,
            bounds: [u32; 4],
            size: usize,
        ) -> (Vec<u8>, usize) {
            let [x0, z0, x1, z1] = bounds;
            let mut pixels = vec![0u8; size * size * 3];
            let mut water = 0usize;
            for py in 0..size {
                let z = (z0 + (z1 - z0) * py as u32 / (size as u32 - 1)) as u16;
                for px in 0..size {
                    let x = (x0 + (x1 - x0) * px as u32 / (size as u32 - 1)) as u16;
                    let h = i32::from(terrain.height_at([x, z]));
                    let gx = i32::from(terrain.height_at([x.saturating_add(128), z]))
                        - i32::from(terrain.height_at([x.saturating_sub(128), z]));
                    let gz = i32::from(terrain.height_at([x, z.saturating_add(128)]))
                        - i32::from(terrain.height_at([x, z.saturating_sub(128)]));
                    if h <= plane {
                        water += 1;
                    }
                    let color = shade(h, plane, gx, gz);
                    let at = (py * size + px) * 3;
                    pixels[at..at + 3].copy_from_slice(&color);
                }
            }
            (pixels, water)
        }

        fn blot(pixels: &mut [u8], size: usize, cx: i32, cy: i32, half: i32, color: [u8; 3]) {
            for dy in -half..=half {
                for dx in -half..=half {
                    let (x, y) = (cx + dx, cy + dy);
                    if x < 0 || y < 0 || x >= size as i32 || y >= size as i32 {
                        continue;
                    }
                    let at = (y as usize * size + x as usize) * 3;
                    pixels[at..at + 3].copy_from_slice(&color);
                }
            }
        }

        fn draw_markers(
            pixels: &mut [u8],
            size: usize,
            bounds: [u32; 4],
            markers: &[(u16, u16, [u8; 3], bool)],
        ) {
            let [x0, z0, x1, z1] = bounds;
            let project = |x: u16, z: u16| {
                if u32::from(x) < x0 || u32::from(x) > x1 || u32::from(z) < z0 || u32::from(z) > z1
                {
                    return None;
                }
                Some([
                    ((u32::from(x) - x0) * (size as u32 - 1) / (x1 - x0)) as i32,
                    ((u32::from(z) - z0) * (size as u32 - 1) / (z1 - z0)) as i32,
                ])
            };
            for (x, z, color, egg) in markers {
                let Some([cx, cy]) = project(*x, *z) else {
                    continue;
                };
                if *egg {
                    blot(pixels, size, cx, cy, 3, [255, 255, 255]);
                }
                blot(pixels, size, cx, cy, 2, *color);
            }
        }

        let out = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(".tmp/pelagia/survey");
        std::fs::create_dir_all(&out).unwrap();
        let full_bounds = [0, 0, 65535, 65535];
        let colony_bounds = [26000, 18000, 43000, 36500];
        let crater_bounds = [43000, 24000, 58000, 47000];
        let size = 1024;
        let (mut full, full_water) = render(terrain, plane, full_bounds, size);
        draw_markers(&mut full, size, full_bounds, &markers);
        let (mut colony, _) = render(terrain, plane, colony_bounds, size);
        draw_markers(&mut colony, size, colony_bounds, &markers);
        let (mut crater, _) = render(terrain, plane, crater_bounds, size);
        draw_markers(&mut crater, size, crater_bounds, &markers);
        let fraction = full_water as f64 / (size * size) as f64;
        assert!(
            (0.5..0.95).contains(&fraction),
            "world 1 stays a water world, water fraction {fraction}"
        );
        for (name, pixels) in [
            ("w01_terrain.png", full),
            ("w01_colony.png", colony),
            ("w01_crater.png", crater),
        ] {
            let file = std::fs::File::create(out.join(name)).unwrap();
            let mut encoder = png::Encoder::new(file, size as u32, size as u32);
            encoder.set_color(png::ColorType::Rgb);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&pixels).unwrap();
            writer.finish().unwrap();
        }
        let mut legend = String::from("Pelagia (world 1) survey: marker colors and counts\n");
        for (category, count) in &counts {
            legend.push_str(&format!("{category}: {count}\n"));
        }
        legend.push_str("egg-red/egg-blue: the port's crystals (white outline)\n");
        legend.push_str(&format!("water fraction: {fraction:.3}\n"));
        std::fs::write(out.join("legend.txt"), legend).unwrap();
    }
}
