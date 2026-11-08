# RecommendationSeedObject

## Properties

Name | Type | Description | Notes
------------ | ------------- | ------------- | -------------
**after_filtering_size** | Option<**i32**> | The number of tracks available after min\\_\\* and max\\_\\* filters have been applied.  | [optional]
**after_relinking_size** | Option<**i32**> | The number of tracks available after relinking for regional availability.  | [optional]
**href** | Option<**String**> | A link to the full track or artist data for this seed. For tracks this will be a link to a Track Object. For artists a link to an Artist Object. For genre seeds, this value will be `null`.  | [optional]
**id** | Option<**String**> | The id used to select this seed. This will be the same as the string used in the `seed_artists`, `seed_tracks` or `seed_genres` parameter.  | [optional]
**initial_pool_size** | Option<**i32**> | The number of recommended tracks available for this seed.  | [optional]
**r#type** | Option<**String**> | The entity type of this seed. One of `artist`, `track` or `genre`.  | [optional]

[[Back to Model list]](../README.md#documentation-for-models) [[Back to API list]](../README.md#documentation-for-api-endpoints) [[Back to README]](../README.md)


