# CurrentlyPlayingContextObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**device** | Option<[**models::DeviceObject**](DeviceObject.md)> | The device that is currently active.  | [optional]
**repeat_state** | Option<**String**> | off, track, context | [optional]
**shuffle_state** | Option<**bool**> | If shuffle is on or off. | [optional]
**context** | Option<[**models::ContextObject**](ContextObject.md)> | A Context Object. Can be `null`. | [optional]
**timestamp** | Option<**i64**> | Unix Millisecond Timestamp when playback state was last changed (play, pause, skip, scrub, new song, etc.). | [optional]
**progress_ms** | Option<**i32**> | Progress into the currently playing track or episode. Can be `null`. | [optional]
**is_playing** | Option<**bool**> | If something is currently playing, return `true`. | [optional]
**item** | Option<[**models::QueueObjectCurrentlyPlaying**](QueueObject_currently_playing.md)> |  | [optional]
**currently_playing_type** | Option<**String**> | The object type of the currently playing item. Can be one of `track`, `episode`, `ad` or `unknown`.  | [optional]
**actions** | Option<[**models::DisallowsObject**](DisallowsObject.md)> | Allows to update the user interface based on which playback actions are available within the current context.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


