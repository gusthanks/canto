# ChapterBase

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**audio_preview_url** | Option<**String**> | A URL to a 30 second preview (MP3 format) of the chapter. `null` if not available.  | 
**available_markets** | Option<**Vec<String>**> | A list of the countries in which the chapter can be played, identified by their [ISO 3166-1 alpha-2](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2) code.  | [optional]
**chapter_number** | **i32** | The number of the chapter  | 
**description** | **String** | A description of the chapter. HTML tags are stripped away from this field, use `html_description` field in case HTML tags are needed.  | 
**html_description** | **String** | A description of the chapter. This field may contain HTML tags.  | 
**duration_ms** | **i32** | The chapter length in milliseconds.  | 
**explicit** | **bool** | Whether or not the chapter has explicit content (true = yes it does; false = no it does not OR unknown).  | 
**external_urls** | [**models::ExternalUrlObject**](ExternalUrlObject.md) | External URLs for this chapter.  | 
**href** | **String** | A link to the Web API endpoint providing full details of the chapter.  | 
**id** | **String** | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the chapter.  | 
**images** | [**Vec<models::ImageObject>**](ImageObject.md) | The cover art for the chapter in various sizes, widest first.  | 
**is_playable** | **bool** | True if the chapter is playable in the given market. Otherwise false.  | 
**languages** | **Vec<String>** | A list of the languages used in the chapter, identified by their [ISO 639-1](https://en.wikipedia.org/wiki/ISO_639) code.  | 
**name** | **String** | The name of the chapter.  | 
**release_date** | **String** | The date the chapter was first released, for example `\"1981-12-15\"`. Depending on the precision, it might be shown as `\"1981\"` or `\"1981-12\"`.  | 
**release_date_precision** | **String** | The precision with which `release_date` value is known.  | 
**resume_point** | Option<[**models::ResumePointObject**](ResumePointObject.md)> | The user's most recent position in the chapter. Set if the supplied access token is a user token and has the scope 'user-read-playback-position'.  | [optional]
**r#type** | **String** | The object type.  | 
**uri** | **String** | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the chapter.  | 
**restrictions** | Option<[**models::ChapterRestrictionObject**](ChapterRestrictionObject.md)> | Included in the response when a content restriction is applied.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


