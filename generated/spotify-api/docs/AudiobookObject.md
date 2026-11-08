# AudiobookObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**authors** | [**Vec<models::AuthorObject>**](AuthorObject.md) | The author(s) for the audiobook.  | 
**available_markets** | **Vec<String>** | A list of the countries in which the audiobook can be played, identified by their [ISO 3166-1 alpha-2](http://en.wikipedia.org/wiki/ISO_3166-1_alpha-2) code.  | 
**copyrights** | [**Vec<models::CopyrightObject>**](CopyrightObject.md) | The copyright statements of the audiobook.  | 
**description** | **String** | A description of the audiobook. HTML tags are stripped away from this field, use `html_description` field in case HTML tags are needed.  | 
**html_description** | **String** | A description of the audiobook. This field may contain HTML tags.  | 
**edition** | Option<**String**> | The edition of the audiobook.  | [optional]
**explicit** | **bool** | Whether or not the audiobook has explicit content (true = yes it does; false = no it does not OR unknown).  | 
**external_urls** | [**models::ExternalUrlObject**](ExternalUrlObject.md) | External URLs for this audiobook.  | 
**href** | **String** | A link to the Web API endpoint providing full details of the audiobook.  | 
**id** | **String** | The [Spotify ID](/documentation/web-api/concepts/spotify-uris-ids) for the audiobook.  | 
**images** | [**Vec<models::ImageObject>**](ImageObject.md) | The cover art for the audiobook in various sizes, widest first.  | 
**languages** | **Vec<String>** | A list of the languages used in the audiobook, identified by their [ISO 639](https://en.wikipedia.org/wiki/ISO_639) code.  | 
**media_type** | **String** | The media type of the audiobook.  | 
**name** | **String** | The name of the audiobook.  | 
**narrators** | [**Vec<models::NarratorObject>**](NarratorObject.md) | The narrator(s) for the audiobook.  | 
**publisher** | **String** | The publisher of the audiobook.  | 
**r#type** | **String** | The object type.  | 
**uri** | **String** | The [Spotify URI](/documentation/web-api/concepts/spotify-uris-ids) for the audiobook.  | 
**total_chapters** | **i32** | The number of chapters in this audiobook.  | 
**chapters** | [**serde_json::Value**](.md) | The chapters of the audiobook.  | 

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


