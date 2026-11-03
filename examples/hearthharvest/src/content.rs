//! Authored crops, village orders and fourteen mornings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Weather {
    Sun,
    Rain,
    Heat,
    Frost,
}
impl Weather {
    pub fn name(self) -> &'static str {
        match self {
            Self::Sun => "Sun",
            Self::Rain => "Rain",
            Self::Heat => "Heat",
            Self::Frost => "Frost",
        }
    }
    pub fn advice(self) -> &'static str {
        match self {
            Self::Sun => "Water growing crops before bed.",
            Self::Rain => "Rain waters every plot for free.",
            Self::Heat => "Dry plants take double stress.",
            Self::Frost => "Cold frames let crops grow tonight.",
        }
    }
}
pub(crate) struct Crop {
    pub name: &'static str,
    pub nights: u8,
    pub seed: u32,
    pub sale: u32,
}
pub(crate) const CROPS: [Crop; 3] = [
    Crop {
        name: "Turnip",
        nights: 2,
        seed: 4,
        sale: 10,
    },
    Crop {
        name: "Bean",
        nights: 3,
        seed: 6,
        sale: 18,
    },
    Crop {
        name: "Pumpkin",
        nights: 4,
        seed: 9,
        sale: 30,
    },
];
pub(crate) struct Order {
    pub name: &'static str,
    pub opens: u8,
    pub deadline: u8,
    pub needs: [u32; 3],
    pub reward: u32,
    pub letter: &'static str,
}
pub(crate) const ORDERS: [Order; 3] = [
    Order {
        name: "Soup Night",
        opens: 1,
        deadline: 5,
        needs: [4, 0, 0],
        reward: 45,
        letter: "Mara's kitchen has a place at the table for everyone. Bring four turnips for the first shared supper.",
    },
    Order {
        name: "Founders' Table",
        opens: 6,
        deadline: 9,
        needs: [2, 3, 0],
        reward: 80,
        letter: "The bridge builders return on day six. Two turnips and three beans will feed their celebration.",
    },
    Order {
        name: "Lantern Feast",
        opens: 10,
        deadline: 14,
        needs: [2, 2, 3],
        reward: 120,
        letter: "Three pumpkins become lanterns; two turnips and two beans fill the final feast. Keep 80 coins to repair the village oven.",
    },
];
pub(crate) struct Morning {
    pub weather: Weather,
    pub letter: &'static str,
}
pub(crate) const MORNINGS: [Morning; 14] = [
    Morning {
        weather: Weather::Sun,
        letter: "Grandma left twelve plots and a little hope. Plant four turnips, water them, and read Mara's order.",
    },
    Morning {
        weather: Weather::Rain,
        letter: "Rain on the roof! New seeds are watered too. Beans need three growing nights; pumpkins need four.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "The first turnips should be ready. Save four for Soup Night before selling your surplus.",
    },
    Morning {
        weather: Weather::Heat,
        letter: "A hot wind comes from the ridge. Unwatered crops gain two stress tonight; three stress means wilt.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "Soup Night is tonight. Deliver four turnips at the village board before you sleep.",
    },
    Morning {
        weather: Weather::Rain,
        letter: "The founders arrive. Their table needs two turnips and three beans by day nine.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "The garden is finding its rhythm. Compost tired soil; a brass can waters three neighboring plots.",
    },
    Morning {
        weather: Weather::Heat,
        letter: "Another dry wind. Protect your pumpkin patch and put a few coins aside for the village oven.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "Last call for the Founders' Table. Frost arrives tomorrow; cold frames protect growing crops.",
    },
    Morning {
        weather: Weather::Frost,
        letter: "The first frost silvers the fence. Unprotected crops pause growth tonight, but watered crops stay safe.",
    },
    Morning {
        weather: Weather::Rain,
        letter: "The thaw smells of earth. The Lantern Feast needs two turnips, two beans and three pumpkins.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "Two nights remain after today. Fresh turnips can still be ready for the festival morning.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "Lantern makers hang ribbons on the barn. Sell surplus produce, but save the feast ingredients.",
    },
    Morning {
        weather: Weather::Sun,
        letter: "Festival day. Deliver the last order and keep 80 coins for the oven; then sleep to gather the village.",
    },
];
pub(crate) const TOOL_NAMES: [&str; 3] = ["Brass Can", "Work Boots", "Cold Frames"];
pub(crate) const TOOL_PRICES: [u32; 3] = [45, 55, 70];
pub(crate) const TOOL_DETAILS: [&str; 3] = [
    "Water the selected plot, its right neighbor and its lower neighbor for one energy.",
    "Gain six extra energy every morning, including the day you buy these boots.",
    "All watered crops keep growing through the day-ten frost. No glass maintenance needed.",
];
