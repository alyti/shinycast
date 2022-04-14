# Shinycast

Self-contained scheduled yt-dlp based podcast mirror generator.

Includes a graphql managment api and a small vue/vuetify frontend for it.
## Features

- **Granular scheduling control per podcast**
    > now you can have an update once a week at 6pm with 4 subsequent 1-hour intermitent updates
- **yt-dlp integration**
    > including fun features such as chapter removal and sponsorblock category marking *wink wink nudge nudge*
- **download queue with worker running on its own schedule**
	> we don't want trouble with media providers so lets throttle our downloads, by default this is 1 video / 5 minutes, but its configurable in same vein as podcast scheduler
- **shiny api and ui for managing podcasts**
    > okay maybe its not a feature for you but this is new and fun to me okay


## Run Locally (with rust/cargo)

Clone the project

```bash
  git clone https://github.com/alyti/shinycast
```

Go to the project directory

```bash
  cd shinycast/app
```

Start the server

```bash
  cargo run
```

Navigate to [localhost:8000/manage](http://localhost:8000/manage) for admin UI.

## Deployment (TBD)

A Docker image is supplied for ease of use in environments like a NAS, and can be installed with the following command:

```sh
docker pull TBD
```
## License

Dual licensed
[MIT License](https://choosealicense.com/licenses/mit/)
[Apache License 2.0](https://choosealicense.com/licenses/apache-2.0/)
## Tech Stack

**Client:** Vue, Vuetify, urql

**Server:** Rust, axum, async-graphql, clokwerk (serde fork), youtube-dl-rs (using yt-dlp)

## Acknowledgements

 - [ticky/playcaster](https://github.com/ticky/playcaster) is a similar project but a lot more minimalist and lightweight
## Feedback

If you have any feedback, please reach out to me via Discord (Alyssa Awoo#0001).
