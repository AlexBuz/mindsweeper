mod client;

use client::Client;
use mindsweeper::server::local::LocalGame;

fn main() {
    yew::Renderer::<Client<LocalGame>>::new().render();
}
