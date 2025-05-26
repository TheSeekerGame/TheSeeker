use crate::map::TilesetTexture;
use crate::render::{ExtractedTileset, ExtractedTilesetTexture};
use bevy::asset::Handle;
use bevy::image::TextureFormatPixelInfo;
use bevy::prelude::{default, Component, Image, Reflect};
use bevy::render::render_resource::{
    AddressMode, Extent3d, FilterMode, ImageCopyTexture, ImageDataLayout, Origin3d, Sampler,
    SamplerDescriptor, Texture, TextureAspect, TextureDescriptor, TextureDimension, TextureUsages,
    TextureView, TextureViewDescriptor, TextureViewDimension,
};
use bevy::render::renderer::{RenderDevice, RenderQueue};

pub struct TextureArray {
    texture: Texture,
    pub(crate) view: TextureView,
    pub(crate) sampler: Sampler,
}

pub fn create_texture_array(
    device: &RenderDevice,
    queue: &RenderQueue,
    extracted_texture: &ExtractedTileset,
) -> TextureArray {
    let texture = device.create_texture(&TextureDescriptor {
        label: Some("tileset_array"),
        size: Extent3d {
            width: extracted_texture.tile_size.x as u32,
            height: extracted_texture.tile_size.y as u32,
            depth_or_array_layers: extracted_texture.tile_count,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: TextureDimension::D2,
        format: extracted_texture.format,
        usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
        view_formats: &[],
    });

    let view = texture.create_view(&TextureViewDescriptor {
        label: Some("tileset_array_view"),
        dimension: Some(TextureViewDimension::D2Array),
        ..default()
    });

    let sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("tileset_array_sampler"),
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
        address_mode_w: AddressMode::ClampToEdge,
        mag_filter: extracted_texture.filtering,
        min_filter: extracted_texture.filtering,
        mipmap_filter: FilterMode::Nearest,
        ..default()
    });

    TextureArray {
        texture,
        view,
        sampler,
    }
}

pub fn update_texture_array(
    device: &RenderDevice,
    queue: &RenderQueue,
    texture_array: &TextureArray,
    extracted_texture: &ExtractedTileset,
) {
    //println!("updatedint texture array");
    // Write the texture data to the GPU
    match &extracted_texture.texture {
        ExtractedTilesetTexture::Single(image_data) => {
            let image_width = extracted_texture.texture_size.x as u32;
            let image_height = extracted_texture.texture_size.y as u32;
            let tile_width = extracted_texture.tile_size.x as u32;
            let tile_height = extracted_texture.tile_size.y as u32;
            let tiles_x = image_width / tile_width;
            let tiles_y = image_height / tile_height;
            let bytes_per_pixel = image_data.texture_descriptor.format.pixel_size() as u32;
            let total_tiles = tiles_x * tiles_y;

            if extracted_texture.tile_count as u32 != total_tiles {
                println!("Warning: Extracted tile count ({}) doesn't match the calculated tile count ({})",
                         extracted_texture.tile_count, total_tiles);
            }

            let mut tile_data = vec![0u8; (tile_width * tile_height * bytes_per_pixel) as usize];

            for tile_index in 0..std::cmp::min(extracted_texture.tile_count, total_tiles as u32) {
                let tile_x = (tile_index % tiles_x) * tile_width;
                let tile_y = (tile_index / tiles_x) * tile_height;

                // Extract tile data
                for y in 0..tile_height {
                    let src_start = ((tile_y + y) * image_width + tile_x) * bytes_per_pixel;
                    let src_end = src_start + (tile_width * bytes_per_pixel);

                    let dst_start = ((tile_height - 1 - y) * tile_width * bytes_per_pixel) as usize;
                    let dst_end = dst_start + (tile_width * bytes_per_pixel) as usize;

                    if src_end as usize > image_data.data.len() {
                        println!(
                            "Error: Tile data exceeds image bounds for tile {}",
                            tile_index
                        );
                        break;
                    }

                    tile_data[dst_start..dst_end]
                        .copy_from_slice(&image_data.data[src_start as usize..src_end as usize]);
                }

                queue.write_texture(
                    ImageCopyTexture {
                        texture: &texture_array.texture,
                        mip_level: 0,
                        origin: Origin3d {
                            x: 0,
                            y: 0,
                            z: tile_index,
                        },
                        aspect: TextureAspect::All,
                    },
                    &tile_data,
                    ImageDataLayout {
                        offset: 0,
                        bytes_per_row: Some(tile_width * bytes_per_pixel),
                        rows_per_image: Some(tile_height),
                    },
                    Extent3d {
                        width: tile_width,
                        height: tile_height,
                        depth_or_array_layers: 1,
                    },
                );
            }
        }
        ExtractedTilesetTexture::Vector(image_data_vec) => {
            for (i, image_data) in image_data_vec.iter().enumerate() {
                unimplemented!()
            }
        }
        ExtractedTilesetTexture::TextureContainer(image_data) => {
            unimplemented!()
        }
    }
}
