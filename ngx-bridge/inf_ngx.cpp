// The port's bridge to NVIDIA NGX: DLSS Super Resolution and Ray
// Reconstruction for the traced world.
//
// NGX's Vulkan API ships only as an MSVC static library, and the game builds
// with the GNU toolchain, so this DLL links NGX and exposes a few C functions
// that the game loads at run time (crates/inf-game/src/vk_device/dlss.rs),
// as it loads FSR's library. Build it with scripts/build-upscalers.ps1
// against the DLSS SDK; NGX then finds nvngx_dlss.dll and nvngx_dlssd.dll
// beside the executable.

#include <vulkan/vulkan.h>

#include "nvsdk_ngx_helpers.h"
#include "nvsdk_ngx_helpers_vk.h"
// After the Super Resolution helpers, whose G-buffer type it uses.
#include "nvsdk_ngx_helpers_dlssd_vk.h"

#define BRIDGE extern "C" __declspec(dllexport)

// The port's own project ID: NGX asks custom engines for a GUID-like one.
static const char *const PROJECT_ID = "4c571895-7922-4769-813c-e5e164e46c4d";
static const char *const ENGINE_VERSION = "1.0";

// The features, as the game names them.
static const unsigned SUPER_RESOLUTION = 0;
static const unsigned RAY_RECONSTRUCTION = 1;

typedef void (*InfNgxLog)(const char *message);

// One image the game hands NGX.
struct InfNgxImage
{
    VkImage image;
    VkImageView view;
    VkFormat format;
    unsigned width;
    unsigned height;
};

// One frame's inputs. Ray Reconstruction also reads the albedos and the
// normals (roughness in alpha) and the camera's matrices (row major).
struct InfNgxFrame
{
    InfNgxImage color;
    InfNgxImage depth;
    InfNgxImage motion;
    InfNgxImage output;
    InfNgxImage diffuse_albedo;
    InfNgxImage specular_albedo;
    InfNgxImage normal_roughness;
    float jitter[2];
    unsigned render[2];
    int reset;
    float frame_ms;
    float world_to_view[16];
    float view_to_clip[16];
};

static InfNgxLog g_log = nullptr;
static NVSDK_NGX_Parameter *g_capabilities = nullptr;
static NVSDK_NGX_Parameter *g_parameters = nullptr;

static void NVSDK_CONV forward_log(const char *message, NVSDK_NGX_Logging_Level, NVSDK_NGX_Feature)
{
    if (g_log && message)
    {
        g_log(message);
    }
}

static NVSDK_NGX_Feature feature_id(unsigned feature)
{
    return feature == RAY_RECONSTRUCTION ? NVSDK_NGX_Feature_RayReconstruction : NVSDK_NGX_Feature_SuperSampling;
}

static NVSDK_NGX_FeatureCommonInfo common_info()
{
    NVSDK_NGX_FeatureCommonInfo info = {};
    info.LoggingInfo.LoggingCallback = forward_log;
    info.LoggingInfo.MinimumLoggingLevel = NVSDK_NGX_LOGGING_LEVEL_OFF;
    return info;
}

static NVSDK_NGX_FeatureDiscoveryInfo discovery(unsigned feature, const wchar_t *data_path, const NVSDK_NGX_FeatureCommonInfo *common)
{
    NVSDK_NGX_FeatureDiscoveryInfo info = {};
    info.SDKVersion = NVSDK_NGX_Version_API;
    info.FeatureID = feature_id(feature);
    info.Identifier.IdentifierType = NVSDK_NGX_Application_Identifier_Type_Project_Id;
    info.Identifier.v.ProjectDesc.ProjectId = PROJECT_ID;
    info.Identifier.v.ProjectDesc.EngineType = NVSDK_NGX_ENGINE_TYPE_CUSTOM;
    info.Identifier.v.ProjectDesc.EngineVersion = ENGINE_VERSION;
    info.ApplicationDataPath = data_path;
    info.FeatureInfo = common;
    return info;
}

// The instance extensions a feature needs, before the instance exists.
BRIDGE unsigned inf_ngx_instance_extensions(unsigned feature, const wchar_t *data_path, unsigned *count, VkExtensionProperties **properties)
{
    NVSDK_NGX_FeatureCommonInfo common = common_info();
    NVSDK_NGX_FeatureDiscoveryInfo info = discovery(feature, data_path, &common);
    return NVSDK_NGX_VULKAN_GetFeatureInstanceExtensionRequirements(&info, count, properties);
}

// The device extensions a feature needs on this physical device.
BRIDGE unsigned inf_ngx_device_extensions(VkInstance instance, VkPhysicalDevice physical, unsigned feature, const wchar_t *data_path, unsigned *count, VkExtensionProperties **properties)
{
    NVSDK_NGX_FeatureCommonInfo common = common_info();
    NVSDK_NGX_FeatureDiscoveryInfo info = discovery(feature, data_path, &common);
    return NVSDK_NGX_VULKAN_GetFeatureDeviceExtensionRequirements(instance, physical, &info, count, properties);
}

BRIDGE unsigned inf_ngx_init(VkInstance instance, VkPhysicalDevice physical, VkDevice device, PFN_vkGetInstanceProcAddr gipa, PFN_vkGetDeviceProcAddr gdpa, const wchar_t *data_path, InfNgxLog log)
{
    g_log = log;
    NVSDK_NGX_FeatureCommonInfo common = common_info();
    NVSDK_NGX_Result result = NVSDK_NGX_VULKAN_Init_with_ProjectID(PROJECT_ID, NVSDK_NGX_ENGINE_TYPE_CUSTOM, ENGINE_VERSION, data_path, instance, physical, device, gipa, gdpa, &common);
    if (NVSDK_NGX_FAILED(result))
    {
        return result;
    }
    result = NVSDK_NGX_VULKAN_GetCapabilityParameters(&g_capabilities);
    if (NVSDK_NGX_FAILED(result))
    {
        NVSDK_NGX_VULKAN_Shutdown1(device);
        return result;
    }
    result = NVSDK_NGX_VULKAN_AllocateParameters(&g_parameters);
    if (NVSDK_NGX_FAILED(result))
    {
        NVSDK_NGX_VULKAN_DestroyParameters(g_capabilities);
        g_capabilities = nullptr;
        NVSDK_NGX_VULKAN_Shutdown1(device);
    }
    return result;
}

// Whether the driver offers a feature (after init).
BRIDGE int inf_ngx_available(unsigned feature)
{
    int available = 0;
    if (g_capabilities)
    {
        NVSDK_NGX_Parameter_GetI(g_capabilities, feature == RAY_RECONSTRUCTION ? NVSDK_NGX_Parameter_SuperSamplingDenoising_Available : NVSDK_NGX_Parameter_SuperSampling_Available, &available);
    }
    return available;
}

// The render size DLSS asks for at a quality (NVSDK_NGX_PerfQuality_Value)
// for this output size.
BRIDGE unsigned inf_ngx_render_size(int quality, unsigned out_width, unsigned out_height, unsigned *width, unsigned *height)
{
    if (!g_capabilities)
    {
        return NVSDK_NGX_Result_FAIL_NotInitialized;
    }
    unsigned max_width = 0, max_height = 0, min_width = 0, min_height = 0;
    float sharpness = 0.0f;
    return NGX_DLSS_GET_OPTIMAL_SETTINGS(g_capabilities, out_width, out_height, (NVSDK_NGX_PerfQuality_Value)quality, width, height, &max_width, &max_height, &min_width, &min_height, &sharpness);
}

BRIDGE unsigned inf_ngx_create(VkDevice device, VkCommandBuffer command, unsigned feature, int quality, unsigned width, unsigned height, unsigned out_width, unsigned out_height, NVSDK_NGX_Handle **handle)
{
    if (!g_parameters)
    {
        return NVSDK_NGX_Result_FAIL_NotInitialized;
    }
    // Motion in render pixels without the jitter; depth 0 near, 1 far.
    const int flags = NVSDK_NGX_DLSS_Feature_Flags_MVLowRes;
    if (feature == RAY_RECONSTRUCTION)
    {
        NVSDK_NGX_DLSSD_Create_Params create = {};
        create.InDenoiseMode = NVSDK_NGX_DLSS_Denoise_Mode_DLUnified;
        create.InRoughnessMode = NVSDK_NGX_DLSS_Roughness_Mode_Packed;
        create.InUseHWDepth = NVSDK_NGX_DLSS_Depth_Type_HW;
        create.InWidth = width;
        create.InHeight = height;
        create.InTargetWidth = out_width;
        create.InTargetHeight = out_height;
        create.InPerfQualityValue = (NVSDK_NGX_PerfQuality_Value)quality;
        // Preset D, the transformer model, which takes the colour as HDR: on
        // the default model (RR2) the port's pulsing lights drifted far more
        // in colour.
        create.InFeatureCreateFlags = flags | NVSDK_NGX_DLSS_Feature_Flags_IsHDR;
        const char *presets[] = {
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_DLAA,
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_Quality,
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_Balanced,
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_Performance,
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_UltraPerformance,
            NVSDK_NGX_Parameter_RayReconstruction_Hint_Render_Preset_UltraQuality,
        };
        for (const char *preset : presets)
        {
            NVSDK_NGX_Parameter_SetUI(g_parameters, preset, NVSDK_NGX_RayReconstruction_Hint_Render_Preset_D);
        }
        return NGX_VULKAN_CREATE_DLSSD_EXT1(device, command, 1, 1, handle, g_parameters, &create);
    }
    NVSDK_NGX_DLSS_Create_Params create = {};
    create.Feature.InWidth = width;
    create.Feature.InHeight = height;
    create.Feature.InTargetWidth = out_width;
    create.Feature.InTargetHeight = out_height;
    create.Feature.InPerfQualityValue = (NVSDK_NGX_PerfQuality_Value)quality;
    create.InFeatureCreateFlags = flags;
    return NGX_VULKAN_CREATE_DLSS_EXT1(device, command, 1, 1, handle, g_parameters, &create);
}

static NVSDK_NGX_Resource_VK resource(const InfNgxImage &image, bool read_write)
{
    VkImageSubresourceRange range = {VK_IMAGE_ASPECT_COLOR_BIT, 0, 1, 0, 1};
    return NVSDK_NGX_Create_ImageView_Resource_VK(image.view, image.image, range, image.format, image.width, image.height, read_write);
}

BRIDGE unsigned inf_ngx_evaluate(VkCommandBuffer command, NVSDK_NGX_Handle *handle, unsigned feature, const InfNgxFrame *frame)
{
    if (!g_parameters)
    {
        return NVSDK_NGX_Result_FAIL_NotInitialized;
    }
    NVSDK_NGX_Resource_VK color = resource(frame->color, false);
    NVSDK_NGX_Resource_VK depth = resource(frame->depth, false);
    NVSDK_NGX_Resource_VK motion = resource(frame->motion, false);
    NVSDK_NGX_Resource_VK output = resource(frame->output, true);
    NVSDK_NGX_Dimensions render = {frame->render[0], frame->render[1]};
    if (feature == RAY_RECONSTRUCTION)
    {
        NVSDK_NGX_Resource_VK diffuse = resource(frame->diffuse_albedo, false);
        NVSDK_NGX_Resource_VK specular = resource(frame->specular_albedo, false);
        NVSDK_NGX_Resource_VK normals = resource(frame->normal_roughness, false);
        float world_to_view[16], view_to_clip[16];
        for (int i = 0; i < 16; i++)
        {
            world_to_view[i] = frame->world_to_view[i];
            view_to_clip[i] = frame->view_to_clip[i];
        }
        NVSDK_NGX_VK_DLSSD_Eval_Params eval = {};
        eval.pInDiffuseAlbedo = &diffuse;
        eval.pInSpecularAlbedo = &specular;
        eval.pInNormals = &normals;
        eval.pInRoughness = &normals;
        eval.pInColor = &color;
        eval.pInOutput = &output;
        eval.pInDepth = &depth;
        eval.pInMotionVectors = &motion;
        eval.InJitterOffsetX = frame->jitter[0];
        eval.InJitterOffsetY = frame->jitter[1];
        eval.InRenderSubrectDimensions = render;
        eval.InReset = frame->reset;
        eval.InMVScaleX = 1.0f;
        eval.InMVScaleY = 1.0f;
        eval.InFrameTimeDeltaInMsec = frame->frame_ms;
        eval.pInWorldToViewMatrix = world_to_view;
        eval.pInViewToClipMatrix = view_to_clip;
        return NGX_VULKAN_EVALUATE_DLSSD_EXT(command, handle, g_parameters, &eval);
    }
    NVSDK_NGX_VK_DLSS_Eval_Params eval = {};
    eval.Feature.pInColor = &color;
    eval.Feature.pInOutput = &output;
    eval.pInDepth = &depth;
    eval.pInMotionVectors = &motion;
    eval.InJitterOffsetX = frame->jitter[0];
    eval.InJitterOffsetY = frame->jitter[1];
    eval.InRenderSubrectDimensions = render;
    eval.InReset = frame->reset;
    eval.InMVScaleX = 1.0f;
    eval.InMVScaleY = 1.0f;
    eval.InFrameTimeDeltaInMsec = frame->frame_ms;
    return NGX_VULKAN_EVALUATE_DLSS_EXT(command, handle, g_parameters, &eval);
}

BRIDGE unsigned inf_ngx_release(NVSDK_NGX_Handle *handle)
{
    return NVSDK_NGX_VULKAN_ReleaseFeature(handle);
}

// After every feature is released, before the device goes.
BRIDGE unsigned inf_ngx_shutdown(VkDevice device)
{
    if (g_parameters)
    {
        NVSDK_NGX_VULKAN_DestroyParameters(g_parameters);
        g_parameters = nullptr;
    }
    if (g_capabilities)
    {
        NVSDK_NGX_VULKAN_DestroyParameters(g_capabilities);
        g_capabilities = nullptr;
    }
    g_log = nullptr;
    return NVSDK_NGX_VULKAN_Shutdown1(device);
}
