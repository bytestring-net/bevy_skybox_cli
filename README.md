# Bevy skybox cli

The pipeline to convert `HDRI -> Bevy`

> [!NOTE]
> This is a 3rd-party tool, not associated with Bevy foundation (yet?).

![image](https://github.com/bytestring-net/bevy_skybox_cli/assets/49441831/8540c2a3-ee2b-4960-b4af-134127f73798)

This repo contains example of Bevy application properly importing skybox, specular and diffuse map.
It also includes CLI to convert `HDRI -> ktx2`.

To run the example, you need to first convert the HDRI in `example/assets` into 3 `.ktx2` files.
That can be done by running `build_assets.sh` which just runs `cargo run --release example/assets/original_4k.hdr` for the CLI.

Due to their combined size of `~200MB` I can't upload them to github.

This project can potentially be adopted into special Bevy Asset plugin, so that Bevy can process HDRI by itself.

You can pick your own HDRI from sites like:
* [Poly haven](https://polyhaven.com/hdris)
* [Poliigon](https://www.poliigon.com/hdrs/free)

## Assets

HDRI used: [kloofendal_48d_partly_cloudy_puresky](https://polyhaven.com/a/kloofendal_48d_partly_cloudy_puresky)

## Contributing

Any contribution submitted by you will be dual licensed as mentioned below, without any additional terms or conditions. If you have the need to discuss this, please contact me.

## Licensing

Released under both [APACHE](./LICENSE-APACHE) and [MIT](./LICENSE-MIT) licenses. Pick one that suits you the most!
